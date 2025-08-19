use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs::{read, write, OpenOptions};
use tokio::time::timeout;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Lockfile {
    pub lockfile: u32,
    #[serde(default)]
    pub sources: Vec<LockSource>,
    #[serde(default)]
    pub artifacts: Vec<LockArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct LockSource {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String, // local|http|git
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(default)]
    pub excludes: Vec<String>,
    pub digest: String,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub artifact: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct LockArtifact {
    pub name: String,
    pub digest: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub systems: Vec<String>,
    #[serde(default)]
    pub deps: Vec<String>,
}

pub async fn load_lock(path: &Path) -> Result<Option<Lockfile>> {
    if !path.exists() {
        return Ok(None);
    }

    let data = read(path).await?;
    let text = String::from_utf8(data)?;
    let lock: Lockfile = toml::from_str(&text)?;
    Ok(Some(lock))
}

/// Atomically save lockfile with proper error handling and backup
pub async fn atomic_save_lock(path: &Path, lock: &Lockfile) -> Result<()> {
    use tokio::fs::{copy, remove_file, rename};

    // Create temporary file path
    let temp_path = path.with_extension("tmp");
    let backup_path = path.with_extension("backup");

    // 1. Serialize lockfile
    let text = toml::to_string_pretty(lock)?;

    // 2. Write to temporary file first
    write(&temp_path, text.as_bytes()).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to write temporary lockfile {}: {}",
            temp_path.display(),
            e
        )
    })?;

    // 3. Create backup of existing file if it exists
    if path.exists() {
        copy(path, &backup_path).await.map_err(|e| {
            anyhow::anyhow!("Failed to create backup {}: {}", backup_path.display(), e)
        })?;
    }

    // 4. Atomically replace the original file
    rename(&temp_path, path).await.map_err(|e| {
        anyhow::anyhow!(
            "Failed to atomically update lockfile {}: {}",
            path.display(),
            e
        )
    })?;

    // 5. Remove backup on success (ignore errors)
    let _ = remove_file(&backup_path).await;

    Ok(())
}

/// File-based lock manager for coordinating lockfile access between processes
pub struct LockfileManager {
    _lock_file: Option<tokio::fs::File>,
    lock_path: PathBuf,
}

impl LockfileManager {
    /// Acquire exclusive lock on lockfile with timeout
    pub async fn acquire(lockfile_path: &Path) -> Result<Self> {
        let lock_path = lockfile_path.with_extension("vorpal_lock");

        // Simple file-based locking: try to create lock file
        // This is a basic implementation - in production we'd want proper file locking
        let acquire_operation = async {
            // Try to create lock file exclusively
            let file = OpenOptions::new()
                .create_new(true) // Fail if file already exists
                .write(true)
                .open(&lock_path)
                .await
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        anyhow::anyhow!("Lockfile is currently in use by another process")
                    } else {
                        anyhow::anyhow!("Failed to acquire lock: {}", e)
                    }
                })?;

            // Write process info to lock file for debugging
            let lock_info = format!(
                "pid:{}\ntime:{}\n",
                std::process::id(),
                chrono::Utc::now().to_rfc3339()
            );

            tokio::fs::write(&lock_path, lock_info).await?;

            anyhow::Ok(Self {
                _lock_file: Some(file),
                lock_path,
            })
        };

        // 30 second timeout for lock acquisition
        timeout(Duration::from_secs(30), acquire_operation)
            .await
            .map_err(|_| anyhow::anyhow!("Timeout waiting for lockfile lock"))?
    }

    /// Perform locked operation with automatic cleanup
    pub async fn with_lock<F, T>(lockfile_path: &Path, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        let _lock = Self::acquire(lockfile_path).await?;
        operation.await
    }
}

impl Drop for LockfileManager {
    fn drop(&mut self) {
        // Clean up lock file on drop (best effort)
        if self.lock_path.exists() {
            let _ = std::fs::remove_file(&self.lock_path);
        }
    }
}

/// Thread-safe lockfile save with coordination
pub async fn save_lock_coordinated(path: &Path, lock: &Lockfile) -> Result<()> {
    LockfileManager::with_lock(path, async { atomic_save_lock(path, lock).await }).await
}
