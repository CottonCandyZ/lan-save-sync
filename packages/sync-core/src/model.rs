use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderConfig {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub excludes: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_history_limit() -> usize {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub device: DeviceConfig,
    #[serde(default = "default_listen")]
    pub listen: String,
    pub api_token: String,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    #[serde(default)]
    pub folders: Vec<FolderConfig>,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
}

fn default_listen() -> String {
    "0.0.0.0:48123".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub modified_unix_ms: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub folder_id: String,
    pub root_hash: String,
    pub generated_at: String,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanDecision {
    InSync,
    Push,
    Pull,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPlan {
    pub folder_id: String,
    pub peer_id: String,
    pub local_hash: String,
    pub remote_hash: String,
    pub base_hash: Option<String>,
    pub decision: PlanDecision,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    Auto,
    Push,
    Pull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyResult {
    pub folder_id: String,
    pub root_hash: String,
    pub backup_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub version: String,
    pub root_hash: String,
    pub created_at: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResponse {
    pub device: DeviceConfig,
    pub folders: Vec<FolderInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckRequest {
    pub folder_id: String,
    pub peer_id: String,
    pub root_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}
