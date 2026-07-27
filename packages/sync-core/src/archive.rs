use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, BufWriter},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use tar::{Archive, Builder, Header};
use tempfile::{NamedTempFile, TempDir};

use crate::{
    manifest::{hash_file, scan, scan_path},
    model::{ApplyResult, FolderConfig, HistoryEntry, Manifest},
};

pub struct PreparedArchive {
    pub file: NamedTempFile,
    pub manifest: Manifest,
}

pub fn prepare_archive(
    folder: &FolderConfig,
    expected_hash: Option<&str>,
    data_dir: &Path,
) -> Result<PreparedArchive> {
    fs::create_dir_all(data_dir)?;
    let manifest = scan(folder)?;
    if let Some(expected) = expected_hash
        && manifest.root_hash != expected
    {
        bail!(
            "source changed before snapshot: expected {expected}, found {}",
            manifest.root_hash
        );
    }

    let file = NamedTempFile::new_in(data_dir)
        .with_context(|| format!("failed to create archive in {}", data_dir.display()))?;
    write_archive(&folder.path, &manifest, file.path())?;
    Ok(PreparedArchive { file, manifest })
}

fn write_archive(root: &Path, manifest: &Manifest, output: &Path) -> Result<()> {
    let writer = BufWriter::new(
        File::create(output)
            .with_context(|| format!("failed to create archive {}", output.display()))?,
    );
    let encoder = GzEncoder::new(writer, Compression::fast());
    let mut builder = Builder::new(encoder);

    for entry in &manifest.files {
        let source = root.join(Path::new(&entry.path));
        let current_hash = hash_file(&source)?;
        if current_hash != entry.sha256 {
            bail!("file changed while creating snapshot: {}", entry.path);
        }
        let metadata = fs::metadata(&source)?;
        if metadata.len() != entry.size {
            bail!("file size changed while creating snapshot: {}", entry.path);
        }

        let mut header = Header::new_gnu();
        header.set_size(metadata.len());
        header.set_mode(0o600);
        header.set_mtime((entry.modified_unix_ms.max(0) as u64) / 1000);
        header.set_cksum();
        let mut input = BufReader::new(File::open(&source)?);
        builder
            .append_data(&mut header, &entry.path, &mut input)
            .with_context(|| format!("failed to archive {}", entry.path))?;
    }
    builder.finish()?;
    let encoder = builder.into_inner()?;
    encoder.finish()?.flush()?;
    Ok(())
}

pub fn apply_archive(
    folder: &FolderConfig,
    archive_path: &Path,
    source_hash: &str,
    expected_current: Option<&str>,
    data_dir: &Path,
    history_limit: usize,
) -> Result<ApplyResult> {
    let current = scan(folder)?;
    if let Some(expected) = expected_current
        && current.root_hash != expected
    {
        bail!(
            "destination changed before apply: expected {expected}, found {}",
            current.root_hash
        );
    }

    let parent = folder
        .path
        .parent()
        .with_context(|| format!("sync path has no parent: {}", folder.path.display()))?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".lan-save-sync-stage-")
        .tempdir_in(parent)?;
    extract_archive(archive_path, stage.path())?;
    let staged_manifest = scan_path(&folder.id, stage.path(), &folder.excludes)?;
    if staged_manifest.root_hash != source_hash {
        bail!(
            "received archive hash mismatch: expected {source_hash}, found {}",
            staged_manifest.root_hash
        );
    }

    let backup_version = if folder.path.exists() {
        Some(create_history_version(
            folder,
            &current,
            data_dir,
            history_limit,
        )?)
    } else {
        None
    };

    swap_directory(&folder.path, stage)?;
    let applied = scan(folder)?;
    if applied.root_hash != source_hash {
        bail!(
            "post-apply verification failed: expected {source_hash}, found {}",
            applied.root_hash
        );
    }
    Ok(ApplyResult {
        folder_id: folder.id.clone(),
        root_hash: applied.root_hash,
        backup_version,
    })
}

fn extract_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let reader = BufReader::new(File::open(archive_path)?);
    let decoder = GzDecoder::new(reader);
    let mut archive = Archive::new(decoder);
    let mut seen = HashSet::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        validate_archive_path(&path)?;
        if !entry.header().entry_type().is_file() {
            bail!("archive contains unsupported entry: {}", path.display());
        }
        let portable = path.to_string_lossy().replace('\\', "/");
        if !seen.insert(portable) {
            bail!("archive contains duplicate path: {}", path.display());
        }
        let output = destination.join(&path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        entry
            .unpack(&output)
            .with_context(|| format!("failed to extract {}", path.display()))?;
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<()> {
    if path.is_absolute() || path.as_os_str().is_empty() {
        bail!("archive contains an unsafe path");
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!("archive contains unsafe path {}", path.display());
        }
    }
    Ok(())
}

fn swap_directory(target: &Path, stage: TempDir) -> Result<()> {
    let parent = target.parent().context("sync target has no parent")?;
    let old = parent.join(format!(
        ".lan-save-sync-old-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let stage_path = stage.keep();

    if target.exists() {
        fs::rename(target, &old).with_context(|| {
            format!(
                "failed to move current data; close the emulator and retry: {}",
                target.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&stage_path, target) {
        if old.exists() {
            let _ = fs::rename(&old, target);
        }
        return Err(error).context("failed to atomically install received snapshot");
    }
    if old.exists() {
        fs::remove_dir_all(&old)
            .with_context(|| format!("snapshot applied but failed to remove {}", old.display()))?;
    }
    Ok(())
}

fn create_history_version(
    folder: &FolderConfig,
    current: &Manifest,
    data_dir: &Path,
    history_limit: usize,
) -> Result<String> {
    let history_dir = history_dir(data_dir, &folder.id);
    fs::create_dir_all(&history_dir)?;
    let version = format!(
        "{}--{}",
        Utc::now().format("%Y%m%dT%H%M%S%.3fZ"),
        &current.root_hash[..12]
    );
    let output = history_dir.join(format!("{version}.tar.gz"));
    write_archive(&folder.path, current, &output)?;
    prune_history(&history_dir, history_limit)?;
    Ok(version)
}

fn history_dir(data_dir: &Path, folder_id: &str) -> PathBuf {
    data_dir.join("versions").join(folder_id)
}

fn prune_history(dir: &Path, history_limit: usize) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("gz"))
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    let remove_count = entries.len().saturating_sub(history_limit);
    for entry in entries.into_iter().take(remove_count) {
        fs::remove_file(entry.path())?;
    }
    Ok(())
}

pub fn list_history(data_dir: &Path, folder_id: &str) -> Result<Vec<HistoryEntry>> {
    let dir = history_dir(data_dir, folder_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(version) = name.strip_suffix(".tar.gz") else {
            continue;
        };
        let (created_at, hash) = version.split_once("--").unwrap_or((version, "unknown"));
        result.push(HistoryEntry {
            version: version.to_owned(),
            root_hash: hash.to_owned(),
            created_at: created_at.to_owned(),
            size: entry.metadata()?.len(),
        });
    }
    result.sort_by(|a, b| b.version.cmp(&a.version));
    Ok(result)
}

pub fn history_archive_path(data_dir: &Path, folder_id: &str, version: &str) -> Result<PathBuf> {
    crate::config::validate_id(version, "version")?;
    let path = history_dir(data_dir, folder_id).join(format!("{version}.tar.gz"));
    if !path.is_file() {
        bail!("history version '{version}' was not found");
    }
    Ok(path)
}

pub fn inspect_archive(
    archive_path: &Path,
    folder: &FolderConfig,
    data_dir: &Path,
) -> Result<Manifest> {
    fs::create_dir_all(data_dir)?;
    let temp = TempDir::new_in(data_dir)?;
    extract_archive(archive_path, temp.path())?;
    scan_path(&folder.id, temp.path(), &folder.excludes)
}

use std::io::Write;

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn folder(path: PathBuf) -> FolderConfig {
        FolderConfig {
            id: "save".into(),
            name: "Save".into(),
            path,
            enabled: true,
            excludes: vec![],
        }
    }

    #[test]
    fn archive_round_trip_and_backup() {
        let temp = tempdir().unwrap();
        let source = folder(temp.path().join("source"));
        let destination = folder(temp.path().join("destination"));
        fs::create_dir_all(source.path.join("slot")).unwrap();
        fs::create_dir_all(destination.path.join("slot")).unwrap();
        fs::write(source.path.join("slot/save.dat"), b"new").unwrap();
        fs::write(destination.path.join("slot/save.dat"), b"old").unwrap();

        let prepared = prepare_archive(&source, None, temp.path()).unwrap();
        let current = scan(&destination).unwrap();
        let result = apply_archive(
            &destination,
            prepared.file.path(),
            &prepared.manifest.root_hash,
            Some(&current.root_hash),
            temp.path(),
            20,
        )
        .unwrap();

        assert!(result.backup_version.is_some());
        assert_eq!(
            fs::read(destination.path.join("slot/save.dat")).unwrap(),
            b"new"
        );
        assert_eq!(list_history(temp.path(), "save").unwrap().len(), 1);
    }
}
