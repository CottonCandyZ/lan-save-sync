use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    bases: BTreeMap<String, String>,
}

#[derive(Clone)]
pub struct StateStore {
    path: Arc<PathBuf>,
    value: Arc<Mutex<PersistedState>>,
}

impl StateStore {
    pub fn load(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir)
            .with_context(|| format!("failed to create data directory {}", data_dir.display()))?;
        let path = data_dir.join("state.json");
        let value = if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read state {}", path.display()))?;
            serde_json::from_str(&raw)
                .with_context(|| format!("invalid state file {}", path.display()))?
        } else {
            PersistedState::default()
        };
        Ok(Self {
            path: Arc::new(path),
            value: Arc::new(Mutex::new(value)),
        })
    }

    pub fn get_base(&self, folder_id: &str, peer_id: &str) -> Option<String> {
        let key = state_key(folder_id, peer_id);
        self.value
            .lock()
            .expect("state lock poisoned")
            .bases
            .get(&key)
            .cloned()
    }

    pub fn set_base(&self, folder_id: &str, peer_id: &str, root_hash: &str) -> Result<()> {
        let mut state = self.value.lock().expect("state lock poisoned");
        state
            .bases
            .insert(state_key(folder_id, peer_id), root_hash.to_owned());
        let encoded = serde_json::to_vec_pretty(&*state)?;
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, encoded)
            .with_context(|| format!("failed to write state {}", temp.display()))?;
        if self.path.exists() {
            fs::remove_file(self.path.as_ref()).with_context(|| {
                format!("failed to replace state {}", self.path.as_ref().display())
            })?;
        }
        fs::rename(&temp, self.path.as_ref())
            .with_context(|| format!("failed to commit state {}", self.path.as_ref().display()))?;
        Ok(())
    }
}

fn state_key(folder_id: &str, peer_id: &str) -> String {
    format!("{folder_id}\0{peer_id}")
}
