use crate::command::store::{hashes::hash_files, paths::get_file_paths};
use anyhow::{bail, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::{read, write};
use tonic::Code::NotFound;
use vorpal_sdk::api::archive::{archive_service_client::ArchiveServiceClient, ArchivePullRequest};

#[derive(Subcommand)]
pub enum CommandLock {
    /// Generate or refresh Vorpal.lock without building
    Generate {
        /// Project root (where Vorpal.toml lives)
        #[arg(default_value = ".")]
        context: PathBuf,
    },

    /// Verify Vorpal.lock against local cache/inputs
    Verify {
        #[arg(default_value = ".")]
        context: PathBuf,
    },

    /// Update specific lock entries (by name) or all
    Update {
        #[arg(default_value = ".")]
        context: PathBuf,

        /// Optional source or artifact name to refresh
        #[arg(long)]
        name: Option<String>,
    },
}

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

pub async fn save_lock(path: &Path, lock: &Lockfile) -> Result<()> {
    let text = toml::to_string(lock)?;
    write(path, text.as_bytes()).await?;
    Ok(())
}

pub async fn run(cmd: &CommandLock, registry: &str) -> Result<()> {
    match cmd {
        CommandLock::Generate { context } => generate(context).await,
        CommandLock::Verify { context } => verify(context, registry).await,
        CommandLock::Update { context, name } => update(context, name.clone()).await,
    }
}

async fn generate(context: &PathBuf) -> Result<()> {
    let lock_path = Path::new(context).join("Vorpal.lock");

    // Initialize minimal lockfile if missing; future work populates content.
    if load_lock(&lock_path).await?.is_none() {
        let lock = Lockfile {
            lockfile: 1,
            sources: vec![],
            artifacts: vec![],
        };
        save_lock(&lock_path, &lock).await?;
        println!("created: {}", lock_path.display());
    } else {
        println!("ok: {}", lock_path.display());
    }

    Ok(())
}

async fn verify(context: &PathBuf, registry: &str) -> Result<()> {
    let lock_path = Path::new(context).join("Vorpal.lock");
    let Some(lock) = load_lock(&lock_path).await? else {
        bail!("missing: {}", lock_path.display());
    };

    // Verify local sources by rehashing inputs
    let mut mismatches = vec![];

    for src in lock.sources.iter().filter(|s| s.kind == "local") {
        let Some(path) = &src.path else { continue };
        let includes = src.includes.clone();
        let excludes = src.excludes.clone();

        let abs = Path::new(context).join(path);
        let files = match get_file_paths(&abs, excludes, includes) {
            Ok(f) => f,
            Err(e) => {
                mismatches.push(format!("{}: enumerate error: {}", src.name, e));
                continue;
            }
        };

        let digest = match hash_files(files) {
            Ok(d) => d,
            Err(e) => {
                mismatches.push(format!("{}: hash error: {}", src.name, e));
                continue;
            }
        };

        if digest != src.digest {
            mismatches.push(format!(
                "{}: digest mismatch: {} != {}",
                src.name, src.digest, digest
            ));
        }
    }

    // Verify remote sources exist in registry by digest
    let mut client = ArchiveServiceClient::connect(registry.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect to registry: {e}"))?;

    for src in lock.sources.iter().filter(|s| s.kind == "http") {
        if src.digest.is_empty() {
            mismatches.push(format!("{}: missing digest in lockfile", src.name));
            continue;
        }

        let req = ArchivePullRequest {
            digest: src.digest.clone(),
        };

        match client.check(req).await {
            Ok(_) => {}
            Err(status) => {
                if status.code() == NotFound {
                    mismatches.push(format!(
                        "{}: digest not present in registry: {}",
                        src.name, src.digest
                    ));
                } else {
                    mismatches.push(format!(
                        "{}: registry error: {}",
                        src.name,
                        status.message()
                    ));
                }
            }
        }
    }

    if mismatches.is_empty() {
        println!("verified: {}", lock_path.display());
        Ok(())
    } else {
        for m in mismatches {
            eprintln!("lock verify: {}", m);
        }
        bail!("verification failed: {}", lock_path.display());
    }
}

async fn update(context: &PathBuf, _name: Option<String>) -> Result<()> {
    // For now, ensure file exists; hook for future selective refresh.
    generate(context).await
}
