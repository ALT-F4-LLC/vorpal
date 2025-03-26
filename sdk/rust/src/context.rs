use crate::{
    artifact::{ArtifactSource, ArtifactSourceKind},
    cli::{Cli, Command},
};
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::{collections::HashMap, path::Path};
use tokio::fs::{read, remove_dir_all, remove_file, write};
use tokio_tar::Archive;
use tonic::{transport::Server, Code::NotFound};
use url::Url;
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{
            Artifact, ArtifactBuildRequest, ArtifactId, ArtifactSourceId, ArtifactStep,
            ArtifactSystem,
        },
        config::v0::{
            config_service_server::{ConfigService, ConfigServiceServer},
            Config, ConfigRequest,
        },
        registry::v0::{
            registry_service_client::RegistryServiceClient, RegistryKind, RegistryPushRequest,
            RegistryRequest,
        },
    },
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zip},
    hashes::hash_files,
    paths::{
        copy_files, get_cache_archive_path, get_file_paths, get_private_key_path, set_timestamps,
    },
    temps::{create_sandbox_dir, create_sandbox_file},
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

#[derive(Clone, Debug, Default)]
pub struct ConfigContext {
    pub artifact_id: HashMap<ArtifactId, Artifact>, // TOOD: make this private
    artifact_source_id: HashMap<ArtifactSourceId, ArtifactSource>,
    port: u16,
    registry: String,
    system: ArtifactSystem,
}

#[derive(Debug, Default)]
pub struct ConfigServer {
    pub config: Config,
    pub context: ConfigContext,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub system: ArtifactSystem,
}

impl ConfigServer {
    pub fn new(context: ConfigContext, config: Config) -> Self {
        Self { context, config }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServer {
    async fn get_config(
        &self,
        _request: tonic::Request<ConfigRequest>,
    ) -> Result<tonic::Response<Config>, tonic::Status> {
        Ok(tonic::Response::new(self.config.clone()))
    }

    async fn get_artifact(
        &self,
        request: tonic::Request<ArtifactId>,
    ) -> Result<tonic::Response<Artifact>, tonic::Status> {
        let request = request.into_inner();

        let artifact = self
            .context
            .get_artifact(request.hash.as_str(), request.name.as_str());

        if artifact.is_none() {
            return Err(tonic::Status::not_found("Artifact input not found"));
        }

        Ok(tonic::Response::new(artifact.unwrap().clone()))
    }
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
        // 1. Check source exists, if hash is set

        if let Some(source_hash) = source.hash.clone() {
            let source_id = ArtifactSourceId {
                hash: source_hash.clone(),
                name: source_name.to_string(),
            };

            if self.artifact_source_id.contains_key(&source_id) {
                return Ok(source_id);
            }

            let registry_request = RegistryRequest {
                hash: source_hash.clone(),
                kind: RegistryKind::ArtifactSource as i32,
                name: source_name.to_string(),
            };

            let registry_host = self.registry.clone();

            let mut registry = RegistryServiceClient::connect(registry_host.to_owned())
                .await
                .expect("failed to connect to registry");

            match registry.exists(registry_request).await {
                Err(status) => {
                    if status.code() != NotFound {
                        bail!("Registry pull error: {:?}", status);
                    }
                }

                Ok(response) => {
                    let response = response.into_inner();

                    let source: ArtifactSource =
                        serde_json::from_str(&response.manifest).map_err(|e| anyhow::anyhow!(e))?;

                    self.artifact_source_id
                        .insert(source_id.clone(), source.clone());

                    return Ok(source_id);
                }
            }
        }

        // 2. Determine kind of source

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

        // 3. Process source path

        let mut source = ArtifactSource {
            excludes: source.excludes.clone(),
            hash: source.hash.clone(),
            includes: source.includes.clone(),
            path: source.path.clone(),
        };

        if source_path_kind == ArtifactSourceKind::Git {
            bail!("`source.{}.path` git not supported", source_name);
        }

        if source_path_kind == ArtifactSourceKind::Local {
            let local_path = Path::new(&source.path).to_path_buf();

            if !local_path.exists() {
                bail!("`source.{}.path` not found: {:?}", source_name, source.path);
            }

            // TODO: make path relevant to the current working directory

            source.path = local_path
                .canonicalize()
                .map_err(|e| anyhow::anyhow!(e))?
                .to_str()
                .unwrap()
                .to_string();
        }

        // 4. Prepare source

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

            // 4a. Download source

            let remote_path = Url::parse(&source.path).map_err(|e| anyhow::anyhow!(e))?;

            if remote_path.scheme() != "http" && remote_path.scheme() != "https" {
                bail!(
                    "source remote scheme not supported: {:?}",
                    remote_path.scheme()
                );
            }

            println!("{} downloading source: {}", source_name, source.path);

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

            // 4b. Extract source

            println!("{} unpacking source: {}", source_name, source.path);

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

            copy_files(
                &local_path,
                local_source_files.clone(),
                &source_sandbox_path,
            )
            .await?;
        }

        // 5. Calculate source hash

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

        // 5a. Set timestamps

        for file_path in source_sandbox_files.clone().into_iter() {
            set_timestamps(&file_path).await?;
        }

        // 5b. Hash source files

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

        // 5c. Package source

        let source_cache_archive_path = get_cache_archive_path(&source_hash, source_name);

        compress_zstd(
            &source_sandbox_path,
            &source_sandbox_files,
            &source_cache_archive_path,
        )
        .await?;

        remove_dir_all(&source_sandbox_path)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        // 5d. push source to registry

        let source_id = ArtifactSourceId {
            hash: source_hash.clone(),
            name: source_name.to_string(),
        };

        let registry_host = self.registry.clone();

        let mut registry = RegistryServiceClient::connect(registry_host.to_owned())
            .await
            .expect("failed to connect to registry");

        let registry_request = RegistryRequest {
            hash: source_hash.clone(),
            kind: RegistryKind::ArtifactSource as i32,
            name: source_name.to_string(),
        };

        match registry.exists(registry_request.clone()).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("Registry pull error: {:?}", status);
                }

                let private_key_path = get_private_key_path();

                if !private_key_path.exists() {
                    bail!("Private key not found: {}", private_key_path.display());
                }

                let source_archive_data = read(&source_cache_archive_path).await?;

                let source_signature =
                    vorpal_notary::sign(private_key_path.clone(), &source_archive_data).await?;

                let source_json = serde_json::to_string(&source).map_err(|e| anyhow::anyhow!(e))?;

                let mut source_push_stream = vec![];

                for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
                    source_push_stream.push(RegistryPushRequest {
                        data: chunk.to_vec(),
                        data_signature: source_signature.clone().to_vec(),
                        hash: source_hash.clone(),
                        kind: RegistryKind::ArtifactSource as i32,
                        manifest: source_json.clone(),
                        name: source_name.to_string(),
                    });
                }

                registry
                    .push(tokio_stream::iter(source_push_stream))
                    .await
                    .expect("failed to push");
            }

            Ok(_) => {}
        }

        println!("{} pushed source: {}", source_name, source_hash);

        self.artifact_source_id
            .insert(source_id.clone(), source.clone());

        Ok(source_id)
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

    pub async fn fetch_artifact(&mut self, name: &str, hash: &str) -> Result<ArtifactId> {
        let registry_host = self.registry.clone();

        let mut registry = RegistryServiceClient::connect(registry_host.to_owned())
            .await
            .expect("failed to connect to registry");

        let registry_request = RegistryRequest {
            hash: hash.to_string(),
            kind: RegistryKind::Artifact as i32,
            name: name.to_string(),
        };

        match registry.exists(registry_request.clone()).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("Registry pull error: {:?}", status);
                }

                bail!("Artifact not found: {:?}", registry_request);
            }

            Ok(response) => {
                let res = response.into_inner();

                let artifact: Artifact =
                    serde_json::from_str(&res.manifest).map_err(|e| anyhow::anyhow!(e))?;

                let artifact_id = ArtifactId {
                    hash: hash.to_string(),
                    name: artifact.name.clone(),
                };

                if !self.artifact_id.contains_key(&artifact_id) {
                    self.artifact_id
                        .insert(artifact_id.clone(), artifact.clone());
                }

                Ok(artifact_id)
            }
        }
    }

    pub fn get_artifact(&self, hash: &str, name: &str) -> Option<&Artifact> {
        let artifact_id = ArtifactId {
            hash: hash.to_string(),
            name: name.to_string(),
        };

        self.artifact_id.get(&artifact_id)
    }

    pub fn get_artifact_source(&self, hash: &str, name: &str) -> Option<&ArtifactSource> {
        let source_id = ArtifactSourceId {
            hash: hash.to_string(),
            name: name.to_string(),
        };

        self.artifact_source_id.get(&source_id)
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
