use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Lockfile {
    pub lockfile: u32,
    #[serde(default)]
    pub sources: Vec<LockSource>,
    #[serde(default)]
    pub artifacts: Vec<LockArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LockSource {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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

pub fn load(path: &Path) -> Result<Option<Lockfile>> {
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read(path)?;
    let text = String::from_utf8(data)?;
    let lock: Lockfile = toml::from_str(&text)?;
    Ok(Some(lock))
}

pub fn save(path: &Path, lock: &Lockfile) -> Result<()> {
    atomic_save(path, lock)
}

/// Atomically save lockfile with proper error handling and backup
pub fn atomic_save(path: &Path, lock: &Lockfile) -> Result<()> {
    use std::fs;

    // Create temporary file path
    let temp_path = path.with_extension("tmp");
    let backup_path = path.with_extension("backup");

    // 1. Serialize lockfile
    let text = toml::to_string_pretty(lock)?;

    // 2. Write to temporary file first
    fs::write(&temp_path, text.as_bytes()).map_err(|e| {
        anyhow::anyhow!(
            "Failed to write temporary lockfile {}: {}",
            temp_path.display(),
            e
        )
    })?;

    // 3. Create backup of existing file if it exists
    if path.exists() {
        fs::copy(path, &backup_path).map_err(|e| {
            anyhow::anyhow!("Failed to create backup {}: {}", backup_path.display(), e)
        })?;
    }

    // 4. Atomically replace the original file
    fs::rename(&temp_path, path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to atomically update lockfile {}: {}",
            path.display(),
            e
        )
    })?;

    // 5. Remove backup on success (ignore errors)
    let _ = fs::remove_file(&backup_path);

    Ok(())
}

pub fn load_from_context(context_path: &Path) -> Result<Option<Lockfile>> {
    let lock_path = context_path.join("Vorpal.lock");
    load(&lock_path)
}

pub fn find_source_digest(
    lock: &Lockfile,
    artifact_name: &str,
    source_name: &str,
    source_path: &str,
) -> Option<String> {
    let is_http = source_path.starts_with("http://") || source_path.starts_with("https://");

    for s in &lock.sources {
        // Match artifact and source name
        if s.name != source_name {
            continue;
        }

        if let Some(art) = &s.artifact {
            if art != artifact_name {
                continue;
            }
        } else {
            continue;
        }

        // Match path or url
        let path_match = !is_http && s.path.as_deref() == Some(source_path);
        let url_match = is_http && s.url.as_deref() == Some(source_path);

        if (path_match || url_match) && !s.digest.is_empty() {
            return Some(s.digest.clone());
        }
    }

    None
}
