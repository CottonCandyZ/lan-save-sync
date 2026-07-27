use std::{
    fs::{self, File, OpenOptions},
    path::Path,
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;

pub struct OperationLock {
    file: File,
}

impl OperationLock {
    pub fn acquire(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir)?;
        let path = data_dir.join("operation.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("failed to open operation lock {}", path.display()))?;
        if file.try_lock_exclusive().is_err() {
            bail!("another sync or restore operation is already running");
        }
        Ok(Self { file })
    }
}

impl Drop for OperationLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
