use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::model::{Config, FolderConfig, PeerConfig};

pub fn load(path: &Path) -> Result<Config> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    let mut de = serde_json::Deserializer::from_str(&raw);
    let config = serde::Deserialize::deserialize(&mut de)
        .with_context(|| format!("invalid config {}", path.display()))?;
    validate(&config)?;
    Ok(config)
}

fn validate(config: &Config) -> Result<()> {
    validate_id(&config.device.id, "device.id")?;
    if config.api_token.len() < 16 || config.api_token.contains("CHANGE_ME") {
        bail!("api_token must be a non-placeholder value with at least 16 characters");
    }
    if config.history_limit == 0 {
        bail!("history_limit must be greater than zero");
    }
    if !config.data_dir.is_absolute() {
        bail!(
            "data_dir must be an absolute path: {}",
            config.data_dir.display()
        );
    }

    let mut folder_ids = HashSet::new();
    for (index, folder) in config.folders.iter().enumerate() {
        validate_folder(folder)?;
        if !folder_ids.insert(&folder.id) {
            bail!("duplicate folder id '{}'", folder.id);
        }
        if paths_overlap(&config.data_dir, &folder.path) {
            bail!(
                "data_dir and folder '{}' must not contain each other",
                folder.id
            );
        }
        for other in &config.folders[..index] {
            if paths_overlap(&folder.path, &other.path) {
                bail!(
                    "folder paths '{}' and '{}' overlap; nested sync roots are unsafe",
                    folder.id,
                    other.id
                );
            }
        }
    }

    let mut peer_ids = HashSet::new();
    for peer in &config.peers {
        validate_peer(peer)?;
        if peer.id == config.device.id {
            bail!("peer id '{}' is the same as this device", peer.id);
        }
        if !peer_ids.insert(&peer.id) {
            bail!("duplicate peer id '{}'", peer.id);
        }
    }
    Ok(())
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        let left = left.to_string_lossy().replace('/', "\\").to_lowercase();
        let right = right.to_string_lossy().replace('/', "\\").to_lowercase();
        let left = left.trim_end_matches('\\');
        let right = right.trim_end_matches('\\');
        left == right
            || left
                .strip_prefix(right)
                .is_some_and(|rest| rest.starts_with('\\'))
            || right
                .strip_prefix(left)
                .is_some_and(|rest| rest.starts_with('\\'))
    } else {
        left.starts_with(right) || right.starts_with(left)
    }
}

fn validate_folder(folder: &FolderConfig) -> Result<()> {
    validate_id(&folder.id, "folder.id")?;
    if folder.name.trim().is_empty() {
        bail!("folder '{}' has an empty name", folder.id);
    }
    if !folder.path.is_absolute() {
        bail!(
            "folder '{}' path must be absolute: {}",
            folder.id,
            folder.path.display()
        );
    }
    Ok(())
}

fn validate_peer(peer: &PeerConfig) -> Result<()> {
    validate_id(&peer.id, "peer.id")?;
    if peer.token.len() < 16 || peer.token.contains("CHANGE_ME") {
        bail!("peer '{}' token must have at least 16 characters", peer.id);
    }
    if !(peer.url.starts_with("http://") || peer.url.starts_with("https://")) {
        bail!("peer '{}' URL must start with http:// or https://", peer.id);
    }
    Ok(())
}

pub fn validate_id(value: &str, field: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        bail!("{field} may only contain ASCII letters, numbers, '.', '-' and '_'");
    }
    Ok(())
}

pub fn find_folder<'a>(config: &'a Config, id: &str) -> Result<&'a FolderConfig> {
    config
        .folders
        .iter()
        .find(|folder| folder.id == id && folder.enabled)
        .with_context(|| format!("enabled folder '{id}' was not found"))
}

pub fn find_peer<'a>(config: &'a Config, id: &str) -> Result<&'a PeerConfig> {
    config
        .peers
        .iter()
        .find(|peer| peer.id == id)
        .with_context(|| format!("peer '{id}' was not found"))
}

pub fn default_config_path(program: &str) -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LanSaveSync")
            .join(format!("{program}.json"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lan-save-sync")
            .join(format!("{program}.json"))
    }
}
