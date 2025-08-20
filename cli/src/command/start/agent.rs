use crate::command::store::{
    archives::{compress_zstd, unpack_zip},
    hashes::hash_files,
    notary,
    paths::{
        copy_files, get_file_paths, get_key_private_path, get_key_public_path, set_timestamps,
    },
    temps::{create_sandbox_dir, create_sandbox_file},
};
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use sha256::digest;
use std::path::Path;
use std::sync::Arc;
use tokio::{
    fs::{read, remove_dir_all, remove_file, write},
    sync::{
        mpsc::{channel, Sender},
        Mutex,
    },
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tar::Archive;
use tonic::{Code, Request, Response, Status};
use tracing::info;
use url::Url;
use vorpal_sdk::api::{
    agent::{agent_service_server::AgentService, PrepareArtifactRequest, PrepareArtifactResponse},
    archive::{
        archive_service_client::ArchiveServiceClient, ArchivePullRequest, ArchivePushRequest,
    },
    artifact::{Artifact, ArtifactSource, ArtifactStep, ArtifactStepSecret},
};

#[derive(PartialEq)]
enum ArtifactSourceType {
    Unknown,
    Local,
    Git,
    Http,
}

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

pub async fn build_source(
    artifact_context: String,
    registry: String,
    source: &ArtifactSource,
    source_cache: Arc<Mutex<std::collections::HashMap<String, String>>>,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<String> {
    let mut client = ArchiveServiceClient::connect(registry.to_owned())
        .await
        .map_err(|err| Status::internal(format!("failed to connect to registry: {err:?}")))?;

    // Determine source type early for caching decisions
    let source_type = match &source.path {
        s if Path::new(s).exists() => ArtifactSourceType::Local,
        s if s.starts_with("git") => ArtifactSourceType::Git,
        s if s.starts_with("http") => ArtifactSourceType::Http,
        _ => ArtifactSourceType::Unknown,
    };

    // For remote sources (HTTP), check cache first and return early if found
    if source_type == ArtifactSourceType::Http {
        let cached_digest = {
            let cache = source_cache.lock().await;
            cache.get(&source.path).cloned()
        };

        if let Some(cached_digest) = cached_digest {
            info!(
                "agent |> source cached: {} -> {}",
                source.name, cached_digest
            );
            return Ok(cached_digest);
        }
    }

    if let Some(digest) = &source.digest {
        let request = ArchivePullRequest {
            digest: digest.to_string(),
        };

        match client.check(request).await {
            Err(status) => {
                if status.code() != Code::NotFound {
                    bail!("registry pull error: {:?}", status);
                }
            }

            Ok(_) => {
                return Ok(digest.to_string());
            }
        }
    }

    // 2. Build source

    if source_type == ArtifactSourceType::Git {
        bail!("'source.{}.path' git not supported", source.name);
    }

    if source_type == ArtifactSourceType::Unknown {
        bail!(
            "'source.{}.path' unknown kind: {:?}",
            source.name,
            source.path
        );
    }

    let source_sandbox = create_sandbox_dir().await?;

    if source_type == ArtifactSourceType::Http {
        // If a digest is provided, we'll later verify it matches the computed digest
        // from the downloaded content. If not provided, proceed and compute it.

        let http_path = Url::parse(&source.path).map_err(|e| anyhow::anyhow!(e))?;

        if http_path.scheme() != "http" && http_path.scheme() != "https" {
            bail!("remote scheme not supported: {:?}", http_path.scheme());
        }

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("download source: {http_path}")),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        let remote_response = reqwest::get(http_path.as_str())
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        if !remote_response.status().is_success() {
            anyhow::bail!("URL not failed: {:?}", remote_response.status());
        }

        let remote_response_bytes = remote_response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        let remote_response_bytes = remote_response_bytes.as_ref();

        let kind = infer::get(remote_response_bytes);

        if kind.is_none() {
            let source_file_name = http_path
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .and_then(|name| if name.is_empty() { None } else { Some(name) })
                .unwrap_or(&source.name);

            let source_file_path = source_sandbox.join(source_file_name);

            write(&source_file_path, remote_response_bytes)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("unpack source: {http_path}")),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        if let Some(kind) = kind {
            match kind.mime_type() {
                "application/gzip" => {
                    let decoder = GzipDecoder::new(remote_response_bytes);
                    let mut archive = Archive::new(decoder);

                    archive
                        .unpack(&source_sandbox)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;

                    // let source_cache_path = source_cache_path.join("...");
                }

                "application/x-bzip2" => {
                    let decoder = BzDecoder::new(remote_response_bytes);
                    let mut archive = Archive::new(decoder);

                    archive
                        .unpack(&source_sandbox)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                }

                "application/x-xz" => {
                    let decoder = XzDecoder::new(remote_response_bytes);
                    let mut archive = Archive::new(decoder);

                    archive
                        .unpack(&source_sandbox)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                }

                "application/zip" => {
                    let archive_sandbox_path = create_sandbox_file(Some("zip")).await?;

                    write(&archive_sandbox_path, remote_response_bytes)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;

                    unpack_zip(&archive_sandbox_path, &source_sandbox).await?;

                    remove_file(&archive_sandbox_path)
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                }

                _ => {
                    bail!(
                        "'source.{}.path' unsupported mime-type detected: {:?}",
                        source.name,
                        source.path
                    );
                }
            }
        }
    }

    if source_type == ArtifactSourceType::Local {
        let artifact_context = Path::new(&artifact_context).to_path_buf();

        if !artifact_context.exists() {
            bail!("artifact not found in: {}", artifact_context.display());
        }

        let local_files = get_file_paths(
            &artifact_context,
            source.excludes.clone(),
            source.includes.clone(),
        )?;

        copy_files(&artifact_context, local_files, &source_sandbox).await?;
    }

    let source_sandbox_files = get_file_paths(
        &source_sandbox,
        source.excludes.clone(),
        source.includes.clone(),
    )?;

    if source_sandbox_files.is_empty() {
        bail!(
            "Artifact 'source.{}.path' no files found: {:?}",
            source.name,
            source.path
        );
    }

    // 3. Sanitize files

    for sandbox_path in source_sandbox_files.clone().into_iter() {
        set_timestamps(&sandbox_path).await?;
    }

    // 4. Hash files

    let source_digest = hash_files(source_sandbox_files.clone())?;

    if let Some(hash) = source.digest.clone() {
        if hash != source_digest {
            bail!(
                "'source.{}.digest' mismatch: {} != {}",
                source.name,
                source_digest,
                hash
            );
        }
    }

    // 5. Push source

    let registry_request = ArchivePullRequest {
        digest: source_digest.clone(),
    };

    if let Err(status) = client.check(registry_request).await {
        if status.code() != Code::NotFound {
            bail!("registry pull error: {:?}", status);
        }

        let source_sandbox_archive = create_sandbox_file(Some("tar.zst")).await?;

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("pack source: {source_digest}")),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        compress_zstd(
            &source_sandbox,
            &source_sandbox_files,
            &source_sandbox_archive,
        )
        .await?;

        let private_key_path = get_key_private_path();

        if !private_key_path.exists() {
            bail!("Private key not found: {}", private_key_path.display());
        }

        let source_archive_data = read(&source_sandbox_archive).await?;

        let source_signature = notary::sign(private_key_path.clone(), &source_archive_data).await?;

        let mut source_stream = vec![];

        for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
            source_stream.push(ArchivePushRequest {
                data: chunk.to_vec(),
                digest: source_digest.clone(),
                signature: source_signature.clone().to_vec(),
            });
        }

        let _ = tx
            .send(Ok(PrepareArtifactResponse {
                artifact: None,
                artifact_digest: None,
                artifact_output: Some(format!("push source: {source_digest}")),
            }))
            .await
            .map_err(|_| Status::internal("failed to send response"));

        client
            .push(tokio_stream::iter(source_stream))
            .await
            .expect("failed to push");

        remove_file(&source_sandbox_archive).await?;
    }

    remove_dir_all(&source_sandbox)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // Cache the digest for remote sources
    if source_type == ArtifactSourceType::Http {
        let mut cache = source_cache.lock().await;
        cache.insert(source.path.clone(), source_digest.clone());
    }

    Ok(source_digest)
}

async fn prepare_artifact(
    registry: String,
    request: Request<PrepareArtifactRequest>,
    source_cache: Arc<Mutex<std::collections::HashMap<String, String>>>,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<(), Status> {
    let request = request.into_inner();

    if request.artifact.is_none() {
        return Err(Status::invalid_argument("'artifact' is required"));
    }

    let artifact = request.artifact.unwrap();

    // No artifact locking - always proceed with normal processing
    info!("agent |> processing artifact: {}", artifact.name);

    // TODO: Check if artifact already exists in the registry

    let public_key_path = get_key_public_path();

    let mut artifact_steps = vec![];

    for step in artifact.steps.iter() {
        let mut secrets = vec![];

        for secret in step.secrets.iter() {
            let value = notary::encrypt(public_key_path.clone(), secret.value.clone())
                .await
                .map_err(|err| Status::internal(format!("failed to encrypt secret: {err}")))?;

            secrets.push(ArtifactStepSecret {
                name: secret.name.clone(),
                value,
            });
        }

        artifact_steps.push(ArtifactStep {
            arguments: step.arguments.clone(),
            artifacts: step.artifacts.clone(),
            entrypoint: step.entrypoint.clone(),
            environments: step.environments.clone(),
            script: step.script.clone(),
            secrets,
        });
    }

    let mut artifact_sources = vec![];

    for source in artifact.sources.into_iter() {
        let source_digest = build_source(
            request.artifact_context.clone(),
            registry.clone(),
            &source,
            source_cache.clone(),
            &tx.clone(),
        )
        .await
        .map_err(|err| Status::internal(format!("{err}")))?;

        let source = ArtifactSource {
            digest: Some(source_digest.to_string()),
            excludes: source.excludes,
            includes: source.includes,
            name: source.name,
            path: source.path,
        };

        artifact_sources.push(source);

        info!("agent |> prepare artifact source: {}", source_digest);

        // Upsert remote source into Vorpal.lock immediately after preparation
        let is_http = artifact_sources
            .last()
            .map(|s| s.path.starts_with("http://") || s.path.starts_with("https://"))
            .unwrap_or(false);

        if is_http {
            // Agent handles lockfile internally using the existing CLI lock module
            // This keeps lockfile logic centralized in one place
            let lock_path = Path::new(&request.artifact_context).join("Vorpal.lock");

            // Load existing lockfile or create new one
            let mut lock = match crate::command::lock::load_lock(&lock_path)
                .await
                .unwrap_or(None)
            {
                Some(l) => l,
                None => crate::command::lock::Lockfile {
                    lockfile: 1,
                    sources: vec![],
                },
            };

            let last = artifact_sources.last().unwrap();

            // Upsert source entry
            if let Some(existing) = lock.sources.iter_mut().find(|s| {
                s.kind == "http"
                    && s.name == last.name
                    && s.artifact.as_deref() == Some(artifact.name.as_str())
                    && s.url.as_deref() == Some(last.path.as_str())
            }) {
                existing.digest = last.digest.clone().unwrap_or_default();
                existing.includes = last.includes.clone();
                existing.excludes = last.excludes.clone();
            } else {
                lock.sources.push(crate::command::lock::LockSource {
                    name: last.name.clone(),
                    kind: "http".to_string(),
                    path: None,
                    url: Some(last.path.clone()),
                    includes: last.includes.clone(),
                    excludes: last.excludes.clone(),
                    digest: last.digest.clone().unwrap_or_default(),
                    rev: None,
                    artifact: Some(artifact.name.clone()),
                });
            }

            lock.sources
                .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

            // Save lockfile - non-fatal if it fails
            if let Err(e) = crate::command::lock::save_lock_coordinated(&lock_path, &lock).await {
                tracing::warn!("Failed to update lockfile {}: {}", lock_path.display(), e);
            } else {
                tracing::info!("Updated lockfile with source: {}", last.name);
            }
        }
    }

    // TODO: explore using combined sources digest for the artifact

    // Store artifact in the registry

    let artifact = Artifact {
        aliases: artifact.aliases,
        name: artifact.name,
        sources: artifact_sources,
        steps: artifact_steps,
        systems: artifact.systems,
        target: artifact.target,
    };

    let artifact_json =
        serde_json::to_vec(&artifact).map_err(|err| Status::internal(format!("{err}")))?;

    let artifact_digest = digest(artifact_json);

    let artifact_response = PrepareArtifactResponse {
        artifact: Some(artifact),
        artifact_digest: Some(artifact_digest.clone()),
        artifact_output: None,
    };

    let _ = tx
        .send(Ok(artifact_response))
        .await
        .map_err(|_| Status::internal("failed to send response"));

    info!("agent |> prepare artifact: {}", artifact_digest);

    Ok(())
}

#[derive(Debug)]
pub struct AgentServer {
    pub registry: String,
    pub source_cache: Arc<Mutex<std::collections::HashMap<String, String>>>,
}

impl AgentServer {
    pub fn new(registry: String) -> Self {
        Self {
            registry,
            source_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[tonic::async_trait]
impl AgentService for AgentServer {
    type PrepareArtifactStream = ReceiverStream<Result<PrepareArtifactResponse, Status>>;

    async fn prepare_artifact(
        &self,
        request: Request<PrepareArtifactRequest>,
    ) -> Result<Response<Self::PrepareArtifactStream>, Status> {
        let (tx, rx) = channel(100);

        let registry = self.registry.clone();

        let source_cache = self.source_cache.clone();

        tokio::spawn(async move {
            if let Err(err) = prepare_artifact(registry, request, source_cache, &tx).await {
                let _ = tx.send(Err(err)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
