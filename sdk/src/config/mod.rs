use crate::config::service::ConfigServer;
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::collections::{BTreeMap, HashMap};
use std::env::consts::{ARCH, OS};
use std::path::Path;
use tokio::fs::{remove_dir_all, remove_file, write};
use tokio_tar::Archive;
use tonic::{transport::Server, Code::NotFound};
use tracing::{info, Level};
use url::Url;
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{
            Artifact, ArtifactBuildRequest, ArtifactId, ArtifactSourceId, ArtifactStep,
            ArtifactSystem,
        },
        config::v0::{config_service_server::ConfigServiceServer, Config},
        registry::v0::{
            registry_service_client::RegistryServiceClient, RegistryKind, RegistryRequest,
        },
    },
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zip},
    hashes::hash_files,
    paths::{copy_files, get_cache_archive_path, get_file_paths, set_timestamps},
    temps::{create_sandbox_dir, create_sandbox_file},
};

pub mod artifact;
pub mod service;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn get_default_system() -> String {
    format!("{}-{}", ARCH, OS)
}

#[derive(Subcommand)]
enum Command {
    Start {
        #[clap(default_value_t = Level::INFO, global = true, long)]
        level: Level,

        #[clap(long, short)]
        port: u16,

        #[clap(default_value = "http://localhost:23151", long, short)]
        registry: String,

        #[arg(default_value_t = get_default_system(), long, short)]
        target: String,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ConfigContext {
    pub artifact_id: HashMap<ArtifactId, Artifact>,
    port: u16,
    registry: String,
    system: ArtifactSystem,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub system: ArtifactSystem,
}

#[derive(Clone, Debug)]
pub struct ArtifactSource {
    pub excludes: Vec<String>,
    pub hash: Option<String>,
    pub includes: Vec<String>,
    pub path: String,
}

#[derive(Debug, PartialEq)]
pub enum ArtifactSourceKind {
    UnknownSourceKind,
    Git,
    Http,
    Local,
}

pub async fn get_context() -> Result<ConfigContext> {
    let args = Cli::parse();

    match args.command {
        Command::Start {
            port,
            registry,
            target,
            ..
        } => {
            let target = get_artifact_system::<ArtifactSystem>(&target);

            if target == ArtifactSystem::UnknownSystem {
                return Err(anyhow::anyhow!("Invalid target system"));
            }

            Ok(ConfigContext::new(port, registry, target))
        }
    }
}

impl ConfigContext {
    pub fn new(port: u16, registry: String, system: ArtifactSystem) -> Self {
        Self {
            artifact_id: HashMap::new(),
            port,
            registry,
            system,
        }
    }

    async fn add_artifact_source(
        &mut self,
        name: &str,
        source: ArtifactSource,
    ) -> Result<ArtifactSourceId> {
        // 1. If source is cached using 'hash', return the source id

        if let Some(hash) = source.hash.clone() {
            // Check if source exists in the registry
            let source_request = RegistryRequest {
                hash: hash.clone(),
                kind: RegistryKind::ArtifactSource as i32,
                name: name.to_string(),
            };

            let registry_host = self.registry.clone();

            let mut registry = RegistryServiceClient::connect(registry_host.to_owned())
                .await
                .expect("failed to connect to registry");

            match registry.exists(source_request.clone()).await {
                Err(status) => {
                    if status.code() != NotFound {
                        bail!("Registry pull error: {:?}", status);
                    }
                }

                Ok(_) => {
                    return Ok(ArtifactSourceId {
                        hash,
                        name: name.to_string(),
                    });
                }
            }

            // Check if source exists in the cache
            let cache_archive_path = get_cache_archive_path(&hash, name);

            if cache_archive_path.exists() {
                return Ok(ArtifactSourceId {
                    hash,
                    name: name.to_string(),
                });
            }
        }

        // 2. If source is not cached, prepare it and cache it

        let source_path_kind = match &source.path {
            s if Path::new(s).exists() => ArtifactSourceKind::Local,
            s if s.starts_with("git") => ArtifactSourceKind::Git,
            s if s.starts_with("http") => ArtifactSourceKind::Http,
            _ => ArtifactSourceKind::UnknownSourceKind,
        };

        if source_path_kind == ArtifactSourceKind::UnknownSourceKind {
            bail!("`source.{}.path` unknown kind: {:?}", name, source.path);
        }

        let source_sandbox_path = create_sandbox_dir().await?;

        if source_path_kind == ArtifactSourceKind::Git {
            bail!("`source.{}.path` git not supported", name);
        }

        if source_path_kind == ArtifactSourceKind::Http {
            if source.hash.is_none() {
                bail!(
                    "`source.{}.hash` required for remote sources: {:?}",
                    name,
                    source.path
                );
            }

            if source.hash.is_some() && source.hash.clone().unwrap() == "" {
                bail!(
                    "`source.{}.hash` empty for remote sources: {:?}",
                    name,
                    source.path
                );
            }

            // Fetch source data from remote path

            let remote_path = Url::parse(&source.path).map_err(|e| anyhow::anyhow!(e))?;

            if remote_path.scheme() != "http" && remote_path.scheme() != "https" {
                bail!(
                    "source remote scheme not supported: {:?}",
                    remote_path.scheme()
                );
            }

            info!("[{}] fetching source -> ({})", name, remote_path.as_str());

            let remote_response = reqwest::get(remote_path.as_str())
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            if !remote_response.status().is_success() {
                anyhow::bail!("source URL not failed: {:?}", remote_response.status());
            }

            let remote_response_bytes = remote_response
                .bytes()
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            let remote_response_bytes = remote_response_bytes.as_ref();

            let kind = infer::get(remote_response_bytes);

            if kind.is_none() {
                let source_file_name = remote_path
                    .path_segments()
                    .and_then(|segments| segments.last())
                    .and_then(|name| if name.is_empty() { None } else { Some(name) })
                    .unwrap_or(name);

                let source_file_path = source_sandbox_path.join(source_file_name);

                write(&source_file_path, remote_response_bytes)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
            }

            // Unpack source data

            if let Some(kind) = kind {
                match kind.mime_type() {
                    "application/gzip" => {
                        let decoder = GzipDecoder::new(remote_response_bytes);
                        let mut archive = Archive::new(decoder);

                        info!(
                            "[{}] unpacking gzip -> {}",
                            name,
                            source_sandbox_path.display(),
                        );

                        archive
                            .unpack(&source_sandbox_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;

                        // let source_cache_path = source_cache_path.join("...");
                    }

                    "application/x-bzip2" => {
                        let decoder = BzDecoder::new(remote_response_bytes);
                        let mut archive = Archive::new(decoder);

                        info!(
                            "[{}] unpacking bzip2 -> {}",
                            name,
                            source_sandbox_path.display(),
                        );

                        archive
                            .unpack(&source_sandbox_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    "application/x-xz" => {
                        let decoder = XzDecoder::new(remote_response_bytes);
                        let mut archive = Archive::new(decoder);

                        info!(
                            "[{}] unpacking xz -> {}",
                            name,
                            source_sandbox_path.display(),
                        );

                        archive
                            .unpack(&source_sandbox_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    "application/zip" => {
                        let archive_sandbox_path = create_sandbox_file(Some("zip")).await?;

                        write(&archive_sandbox_path, remote_response_bytes)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;

                        info!(
                            "[{}] unpacking zip -> {}",
                            name,
                            source_sandbox_path.display(),
                        );

                        unpack_zip(&archive_sandbox_path, &source_sandbox_path).await?;

                        remove_file(&archive_sandbox_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    _ => {
                        bail!(
                            "`source.{}.path` unsupported mime-type detected: {:?}",
                            name,
                            source.path
                        );
                    }
                }
            }
        }

        if source_path_kind == ArtifactSourceKind::Local {
            let local_path = Path::new(&source.path).to_path_buf();

            if !local_path.exists() {
                bail!("`source.{}.path` not found: {:?}", name, source.path);
            }

            let local_source_files = get_file_paths(
                &local_path,
                source.excludes.clone(),
                source.includes.clone(),
            )?;

            copy_files(
                &local_path,
                local_source_files.clone(),
                &source_sandbox_path,
            )
            .await?;
        }

        let source_sandbox_files = get_file_paths(
            &source_sandbox_path,
            source.excludes.clone(),
            source.includes.clone(),
        )?;

        if source_sandbox_files.is_empty() {
            bail!(
                "Artifact `source.{}.path` no files found: {:?}",
                name,
                source.path
            );
        }

        for file_path in source_sandbox_files.clone().into_iter() {
            set_timestamps(&file_path).await?;
        }

        let mut source_hash_default = "untracked".to_string();

        if let Some(hash) = source.hash.clone() {
            source_hash_default = hash;
        }

        info!("[{}] hashing source -> {}", name, source_hash_default);

        let source_hash = hash_files(source_sandbox_files.clone())?;

        if let Some(hash) = source.hash.clone() {
            if hash != source_hash {
                bail!(
                    "`source.{}.hash` mismatch: {} != {}",
                    name,
                    source_hash,
                    hash
                );
            }
        }

        info!("[{}] caching source -> {}", name, source_hash);

        let cache_archive_path = get_cache_archive_path(&source_hash, name);

        if !cache_archive_path.exists() {
            compress_zstd(
                &source_sandbox_path,
                &source_sandbox_files,
                &cache_archive_path,
            )
            .await?;
        }

        remove_dir_all(&source_sandbox_path)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(ArtifactSourceId {
            hash: source_hash,
            name: name.to_string(),
        })
    }

    pub async fn add_artifact(
        &mut self,
        name: &str,
        artifacts: Vec<ArtifactId>,
        source: BTreeMap<&str, ArtifactSource>,
        steps: Vec<ArtifactStep>,
        systems: Vec<&str>,
    ) -> Result<ArtifactId> {
        // 1. Setup sources

        let mut sources = vec![];

        for (source_name, source_args) in source.into_iter() {
            let source = self.add_artifact_source(source_name, source_args).await?;

            sources.push(source);
        }

        // 2. Setup systems

        let mut systems_int = vec![];

        for system in systems {
            let system = get_artifact_system::<ArtifactSystem>(system);

            if system == ArtifactSystem::UnknownSystem {
                bail!("Unsupported system: {}", system.as_str_name());
            }

            systems_int.push(system.into());
        }

        // 3. Setup artifact id

        let artifact = Artifact {
            artifacts,
            name: name.to_string(),
            sources,
            steps,
            systems: systems_int,
        };

        let artifact_manifest = ArtifactBuildRequest {
            artifact: Some(artifact.clone()),
            system: self.system.into(),
        };

        let artifact_manifest_json =
            serde_json::to_string(&artifact_manifest).map_err(|e| anyhow::anyhow!(e))?;

        let artifact_manifest_hash = digest(artifact_manifest_json.as_bytes());

        let artifact_id = ArtifactId {
            hash: artifact_manifest_hash,
            name: artifact.name.clone(),
        };

        if !self.artifact_id.contains_key(&artifact_id) {
            self.artifact_id
                .insert(artifact_id.clone(), artifact.clone());
        }

        Ok(artifact_id)
    }

    pub fn get_artifact(&self, hash: &str, name: &str) -> Option<&Artifact> {
        let artifact_id = ArtifactId {
            hash: hash.to_string(),
            name: name.to_string(),
        };

        self.artifact_id.get(&artifact_id)
    }

    pub fn get_target(&self) -> ArtifactSystem {
        self.system
    }

    pub async fn run(&self, artifacts: Vec<ArtifactId>) -> Result<()> {
        let addr = format!("[::]:{}", self.port)
            .parse()
            .expect("failed to parse address");

        let config = Config { artifacts };

        let context = self.clone();

        let config_service = ConfigServiceServer::new(ConfigServer::new(context, config));

        println!("Config listening: {}", addr);

        Server::builder()
            .add_service(config_service)
            .serve(addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))
    }
}
