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
    let text = toml::to_string(lock)?;
    fs::write(path, text.as_bytes())?;
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

        if path_match || url_match {
            if !s.digest.is_empty() {
                return Some(s.digest.clone());
            }
        }
    }

    None
}
