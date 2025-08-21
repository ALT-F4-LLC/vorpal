use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs::{read, write};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Lockfile {
    pub lockfile: u32,
    #[serde(default)]
    pub sources: Vec<LockSource>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct LockSource {
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(default)]
    pub excludes: Vec<String>,
    pub digest: String,
    #[serde(default)]
    pub platform: String, // Single platform where this source is used (e.g., "aarch64-darwin", "aarch64-linux")
}

pub async fn load_lock(path: &Path) -> Result<Option<Lockfile>> {
    if !path.exists() {
        return Ok(None);
    }

    let lock_data = read(path).await?;
    let lock_text = String::from_utf8(lock_data)?;
    let lock: Lockfile = toml::from_str(&lock_text)?;

    Ok(Some(lock))
}

pub async fn save_lock(path: &Path, lock: &Lockfile) -> Result<()> {
    let data = toml::to_string_pretty(lock)?;

    write(path, data.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write lockfile {}: {}", path.display(), e))
}

pub fn artifact_system_to_platform(system: i32) -> String {
    use vorpal_sdk::api::artifact::ArtifactSystem;

    match ArtifactSystem::try_from(system).unwrap_or(ArtifactSystem::UnknownSystem) {
        ArtifactSystem::Aarch64Darwin => "aarch64-darwin".to_string(),
        ArtifactSystem::Aarch64Linux => "aarch64-linux".to_string(),
        ArtifactSystem::X8664Darwin => "x86_64-darwin".to_string(),
        ArtifactSystem::X8664Linux => "x86_64-linux".to_string(),
        ArtifactSystem::UnknownSystem => "unknown".to_string(),
    }
}
