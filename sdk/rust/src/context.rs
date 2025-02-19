use crate::{
    artifact::{ArtifactSource, ArtifactSourceKind},
    cli::{Cli, Command},
    service::ConfigServer,
};
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::{collections::HashMap, path::Path};
use tokio::fs::{remove_dir_all, remove_file, write};
use tokio_tar::Archive;
use tonic::{transport::Server, Code::NotFound};
use tracing::info;
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

#[derive(Clone, Debug, Default)]
pub struct ConfigContext {
    pub artifact_id: HashMap<ArtifactId, Artifact>, // TOOD: make this private
    artifact_source_id: HashMap<String, ArtifactSourceId>,
    port: u16,
    registry: String,
    system: ArtifactSystem,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub system: ArtifactSystem,
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
            artifact_source_id: HashMap::new(),
            port,
            registry,
            system,
        }
    }

    pub async fn add_artifact_source(
        &mut self,
        source_name: &str,
        source: ArtifactSource,
    ) -> Result<ArtifactSourceId> {
        // 1. If source is cached using '<source-name>-<digest>', return the source id

        // TODO: if any paths are relative, they should be expanded to the artifact's source directory

        let source_path_kind = match &source.path {
            s if Path::new(s).exists() => ArtifactSourceKind::Local,
            s if s.starts_with("git") => ArtifactSourceKind::Git,
            s if s.starts_with("http") => ArtifactSourceKind::Http,
            _ => ArtifactSourceKind::UnknownSourceKind,
        };

        if source_path_kind == ArtifactSourceKind::UnknownSourceKind {
            bail!(
                "`source.{}.path` unknown kind: {:?}",
                source_name,
                source.path
            );
        }

        if source_path_kind == ArtifactSourceKind::Git {
            bail!("`source.{}.path` git not supported", source_name);
        }

        let mut source = ArtifactSource {
            excludes: source.excludes.clone(),
            hash: source.hash.clone(),
            includes: source.includes.clone(),
            path: source.path.clone(),
        };

        if source_path_kind == ArtifactSourceKind::Local {
            let local_path = Path::new(&source.path).to_path_buf();

            if !local_path.exists() {
                bail!("`source.{}.path` not found: {:?}", source_name, source.path);
            }

            source.path = local_path
                .canonicalize()
                .map_err(|e| anyhow::anyhow!(e))?
                .to_str()
                .unwrap()
                .to_string();

            info!(
                "{} canonicalized source: {}",
                source_name,
                local_path.display()
            );
        }

        let source_json = serde_json::to_string(&source).map_err(|e| anyhow::anyhow!(e))?;

        let source_key = format!("{}-{}", source_name, digest(source_json));

        if let Some(source_id) = self.artifact_source_id.get(&source_key) {
            return Ok(source_id.clone());
        }

        // 2. Check if source exists in registry or local cache

        if let Some(hash) = source.hash.clone() {
            let artifact_source_id = ArtifactSourceId {
                hash: hash.clone(),
                name: source_name.to_string(),
            };

            // 2a. Check if source exists in the registry

            let registry_host = self.registry.clone();

            let mut registry = RegistryServiceClient::connect(registry_host.to_owned())
                .await
                .expect("failed to connect to registry");

            let registry_request = RegistryRequest {
                hash: hash.clone(),
                kind: RegistryKind::ArtifactSource as i32,
                name: source_name.to_string(),
            };

            match registry.exists(registry_request.clone()).await {
                Err(status) => {
                    if status.code() != NotFound {
                        bail!("Registry pull error: {:?}", status);
                    }
                }

                Ok(_) => {
                    info!("{} pushed source: {}", source_name, hash);

                    self.artifact_source_id
                        .insert(source_key, artifact_source_id.clone());

                    return Ok(artifact_source_id);
                }
            }

            // 2b. Check if source exists in local cache

            let cache_archive_path = get_cache_archive_path(&hash, source_name);

            if cache_archive_path.exists() {
                info!("{} cached source: {}", source_name, hash);

                self.artifact_source_id
                    .insert(source_key, artifact_source_id.clone());

                return Ok(artifact_source_id);
            }
        }

        // 3. Prepare source if not cached

        let source_sandbox_path = create_sandbox_dir().await?;

        if source_path_kind == ArtifactSourceKind::Http {
            if source.hash.is_none() {
                bail!(
                    "`source.{}.hash` required for remote sources: {:?}",
                    source_name,
                    source.path
                );
            }

            if source.hash.is_some() && source.hash.clone().unwrap() == "" {
                bail!(
                    "`source.{}.hash` empty for remote sources: {:?}",
                    source_name,
                    source.path
                );
            }

            let remote_path = Url::parse(&source.path).map_err(|e| anyhow::anyhow!(e))?;

            if remote_path.scheme() != "http" && remote_path.scheme() != "https" {
                bail!(
                    "source remote scheme not supported: {:?}",
                    remote_path.scheme()
                );
            }

            info!("{} downloading source: {}", source_name, source.path);

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
                    .unwrap_or(source_name);

                let source_file_path = source_sandbox_path.join(source_file_name);

                write(&source_file_path, remote_response_bytes)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
            }

            // Unpack source data

            info!("{} unpacking source: {}", source_name, source.path);

            if let Some(kind) = kind {
                match kind.mime_type() {
                    "application/gzip" => {
                        let decoder = GzipDecoder::new(remote_response_bytes);
                        let mut archive = Archive::new(decoder);

                        archive
                            .unpack(&source_sandbox_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;

                        // let source_cache_path = source_cache_path.join("...");
                    }

                    "application/x-bzip2" => {
                        let decoder = BzDecoder::new(remote_response_bytes);
                        let mut archive = Archive::new(decoder);

                        archive
                            .unpack(&source_sandbox_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    "application/x-xz" => {
                        let decoder = XzDecoder::new(remote_response_bytes);
                        let mut archive = Archive::new(decoder);

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

                        unpack_zip(&archive_sandbox_path, &source_sandbox_path).await?;

                        remove_file(&archive_sandbox_path)
                            .await
                            .map_err(|e| anyhow::anyhow!(e))?;
                    }

                    _ => {
                        bail!(
                            "`source.{}.path` unsupported mime-type detected: {:?}",
                            source_name,
                            source.path
                        );
                    }
                }
            }
        }

        if source_path_kind == ArtifactSourceKind::Local {
            let local_path = Path::new(&source.path).to_path_buf();

            if !local_path.exists() {
                bail!("`source.{}.path` not found: {:?}", source_name, source.path);
            }

            let local_source_files = get_file_paths(
                &local_path,
                source.excludes.clone(),
                source.includes.clone(),
            )?;

            info!(
                "{} copying source: {}",
                source_name,
                local_path.canonicalize().unwrap().display()
            );

            copy_files(
                &local_path,
                local_source_files.clone(),
                &source_sandbox_path,
            )
            .await?;
        }

        // 4. Calculate source hash

        let source_sandbox_files = get_file_paths(
            &source_sandbox_path,
            source.excludes.clone(),
            source.includes.clone(),
        )?;

        if source_sandbox_files.is_empty() {
            bail!(
                "Artifact `source.{}.path` no files found: {:?}",
                source_name,
                source.path
            );
        }

        // 4a. Set timestamps

        for file_path in source_sandbox_files.clone().into_iter() {
            set_timestamps(&file_path).await?;
        }

        info!("{} hashing source: {}", source_name, source.path);

        // 4b. Hash source files

        let source_hash = hash_files(source_sandbox_files.clone())?;

        if let Some(hash) = source.hash.clone() {
            if hash != source_hash {
                bail!(
                    "`source.{}.hash` mismatch: {} != {}",
                    source_name,
                    source_hash,
                    hash
                );
            }
        }

        info!("{} caching source: {}", source_name, source.path);

        let cache_archive_path = get_cache_archive_path(&source_hash, source_name);

        compress_zstd(
            &source_sandbox_path,
            &source_sandbox_files,
            &cache_archive_path,
        )
        .await?;

        remove_dir_all(&source_sandbox_path)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        let id = ArtifactSourceId {
            hash: source_hash,
            name: source_name.to_string(),
        };

        self.artifact_source_id.insert(source_key, id.clone());

        Ok(id)
    }

    pub async fn add_artifact(
        &mut self,
        name: &str,
        artifacts: Vec<ArtifactId>,
        sources: Vec<ArtifactSourceId>,
        steps: Vec<ArtifactStep>,
        systems: Vec<&str>,
    ) -> Result<ArtifactId> {
        // 1. Setup systems

        let mut systems_int = vec![];

        for system in systems {
            let system = get_artifact_system::<ArtifactSystem>(system);

            if system == ArtifactSystem::UnknownSystem {
                bail!("Unsupported system: {}", system.as_str_name());
            }

            systems_int.push(system.into());
        }

        // 2. Setup artifact id

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
