use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::model::{FileEntry, FolderConfig, Manifest};

pub fn scan(folder: &FolderConfig) -> Result<Manifest> {
    scan_path(&folder.id, &folder.path, &folder.excludes)
}

pub fn scan_path(folder_id: &str, root: &Path, excludes: &[String]) -> Result<Manifest> {
    let matcher = build_matcher(excludes)?;
    let mut files = Vec::new();
    let mut portable_names = HashMap::new();

    if root.exists() {
        if !root.is_dir() {
            bail!("sync path is not a directory: {}", root.display());
        }

        for item in WalkDir::new(root).follow_links(false) {
            let item = item.with_context(|| format!("failed to walk {}", root.display()))?;
            let path = item.path();
            if path == root {
                continue;
            }

            let relative = path
                .strip_prefix(root)
                .context("walked path escaped the sync root")?;
            let portable = portable_path(relative)?;
            if is_excluded(&matcher, &portable) {
                if item.file_type().is_dir() {
                    continue;
                }
                continue;
            }
            if item.file_type().is_symlink() {
                bail!("symbolic links are not supported: {}", path.display());
            }
            if !item.file_type().is_file() {
                continue;
            }
            let folded = portable.to_lowercase();
            if let Some(existing) = portable_names.insert(folded, portable.clone()) {
                bail!(
                    "paths differ only by letter case and cannot sync safely to Windows: '{}' and '{}'",
                    existing,
                    portable
                );
            }

            let metadata = item
                .metadata()
                .with_context(|| format!("failed to read metadata for {}", path.display()))?;
            let modified_unix_ms = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as i64)
                .unwrap_or_default();
            files.push(FileEntry {
                path: portable,
                size: metadata.len(),
                modified_unix_ms,
                sha256: hash_file(path)?,
            });
        }
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    let root_hash = root_hash(&files);
    Ok(Manifest {
        folder_id: folder_id.to_owned(),
        root_hash,
        generated_at: Utc::now().to_rfc3339(),
        files,
    })
}

fn build_matcher(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    builder.add(Glob::new(".lan-save-sync-*")?);
    builder.add(Glob::new("**/.lan-save-sync-*")?);
    for pattern in patterns {
        builder.add(
            Glob::new(pattern).with_context(|| format!("invalid exclude pattern '{pattern}'"))?,
        );
    }
    Ok(builder.build()?)
}

fn is_excluded(matcher: &GlobSet, path: &str) -> bool {
    matcher.is_match(path)
}

pub fn portable_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(value) => {
                let value = value
                    .to_str()
                    .with_context(|| format!("path is not valid UTF-8: {}", path.display()))?;
                if value.is_empty() || value == "." || value == ".." {
                    bail!("unsafe path component in {}", path.display());
                }
                parts.push(value);
            }
            _ => bail!("unsafe relative path {}", path.display()),
        }
    }
    Ok(parts.join("/"))
}

pub fn hash_file(path: &Path) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("failed to open file for hashing: {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to hash {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn root_hash(files: &[FileEntry]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"lan-save-sync-manifest-v1\0");
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.size.to_le_bytes());
        hasher.update([0]);
        hasher.update(file.sha256.as_bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn hash_is_stable_and_ignores_modification_time() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("save.dat"), b"hello").unwrap();
        let first = scan_path("game", temp.path(), &[]).unwrap();
        let second = scan_path("game", temp.path(), &[]).unwrap();
        assert_eq!(first.root_hash, second.root_hash);
    }

    #[test]
    fn excludes_matching_files() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("save.dat"), b"hello").unwrap();
        fs::write(temp.path().join("cache.tmp"), b"ignored").unwrap();
        let result = scan_path("game", temp.path(), &["**/*.tmp".into(), "*.tmp".into()]).unwrap();
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "save.dat");
    }
}
