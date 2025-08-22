use crate::command::{
    lock::{artifact_system_to_platform, load_lock, save_lock, LockSource, Lockfile},
    store::{
        archives::{compress_zstd, unpack_zip},
        hashes::get_source_digest,
        notary,
        paths::{
            copy_files, get_file_paths, get_key_ca_path, get_key_service_key_path,
            get_key_service_public_path, set_timestamps,
        },
        temps::{create_sandbox_dir, create_sandbox_file},
    },
};
use anyhow::{bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder, XzDecoder};
use http::uri::{InvalidUri, Uri};
use sha256::digest;
use std::path::Path;
use tokio::{
    fs::{read, remove_dir_all, remove_file, write},
    sync::mpsc::{channel, Sender},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tar::Archive;
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig},
    Code, Request, Response, Status,
};
use tracing::{info, warn};
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
    artifact_update: bool,
    mut client_archive: ArchiveServiceClient<Channel>,
    source: &ArtifactSource,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<String> {
    let source_type = match &source.path {
        s if Path::new(s).exists() => ArtifactSourceType::Local,
        s if s.starts_with("git") => ArtifactSourceType::Git,
        s if s.starts_with("http") => ArtifactSourceType::Http,
        _ => ArtifactSourceType::Unknown,
    };

    if source_type == ArtifactSourceType::Unknown {
        bail!(
            "'source.{}.path' unknown kind: {:?}",
            source.name,
            source.path
        );
    }

    if let Some(digest) = &source.digest {
        let request = ArchivePullRequest {
            digest: digest.to_string(),
        };

        match client_archive.check(request).await {
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

    let source_digest = get_source_digest(source_sandbox_files.clone())?;

    if let Some(digest) = source.digest.clone() {
        if !artifact_update && source_digest != digest {
            bail!(
                "'source.{}.digest' mismatch: {} != {}",
                source.name,
                source_digest,
                digest
            );
        }

        if source_digest == digest {
            info!(
                "agent |> verified source: {} ({})",
                source.name, source_digest
            );
        }
    }

    // 5. Push source

    let registry_request = ArchivePullRequest {
        digest: source_digest.clone(),
    };

    if let Err(status) = client_archive.check(registry_request).await {
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

        let private_key_path = get_key_service_key_path();

        if !private_key_path.exists() {
            bail!("Private key not found: {}", private_key_path.display());
        }

        let source_archive_data = read(&source_sandbox_archive).await?;

        let mut source_stream = vec![];

        for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) {
            source_stream.push(ArchivePushRequest {
                data: chunk.to_vec(),
                digest: source_digest.clone(),
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

        client_archive
            .push(tokio_stream::iter(source_stream))
            .await
            .expect("failed to push");

        remove_file(&source_sandbox_archive).await?;
    }

    remove_dir_all(&source_sandbox)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(source_digest)
}

async fn prepare_artifact(
    registry: String,
    request: Request<PrepareArtifactRequest>,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<(), Status> {
    let request = request.into_inner();

    if request.artifact.is_none() {
        return Err(Status::invalid_argument("'artifact' is required"));
    }

    let artifact = request.artifact.unwrap();

    // TODO: Check if artifact already exists in the registry

    let public_key_path = get_key_service_public_path();

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

    // Load lockfile to hydrate source digests before processing
    let lock_path = Path::new(&request.artifact_context).join("Vorpal.lock");
    let lock_file = load_lock(&lock_path).await.unwrap_or(None);

    let mut artifact_sources = vec![];

    let target_platform = artifact_system_to_platform(artifact.target);

    for mut source in artifact.sources.into_iter() {
        if let Some(ref lock) = lock_file {
            if let Some(lock_source) = lock
                .sources
                .iter()
                .find(|s| s.name == source.name && s.platform == target_platform)
            {
                let mut source_changed = false;

                if source.includes != lock_source.includes {
                    source_changed = true;
                }

                if source.excludes != lock_source.excludes {
                    source_changed = true;
                }

                if source.path != lock_source.path {
                    source_changed = true;
                }

                if !request.artifact_update {
                    // Check if source path changed in lockfile

                    if source_changed {
                        return Err(Status::failed_precondition(format!(
                            "source '{}' changed in lockfile: {:?} -> {:?}",
                            source.name, source.path, lock_source.path
                        )));
                    }

                    // Checks if digests match configuration

                    if let Some(source_digest) = source.digest.as_ref() {
                        if lock_source.digest != *source_digest {
                            return Err(Status::failed_precondition(format!(
                                "source '{}' digest changed in lockfile: {:?} -> {:?}",
                                source.name,
                                source.digest.as_ref().unwrap(),
                                lock_source.digest
                            )));
                        }
                    }
                }

                // Check if digest exists in lockfile

                if !lock_source.digest.is_empty() && !source_changed {
                    source.digest = Some(lock_source.digest.clone());

                    info!(
                        "agent |> hydrated source: {} ({}) -> {}",
                        source.name, target_platform, lock_source.digest
                    );
                }
            }
        }

        let ca_pem_path = get_key_ca_path();

        if !ca_pem_path.exists() {
            return Err(Status::internal(format!(
                "CA certificate not found: {}",
                ca_pem_path.display()
            )));
        }

        let service_ca_pem = read(ca_pem_path)
            .await
            .expect("failed to read CA certificate");

        let service_ca = Certificate::from_pem(service_ca_pem);

        let service_tls = ClientTlsConfig::new()
            .ca_certificate(service_ca)
            .domain_name("localhost");

        let service_uri = registry.parse::<Uri>().map_err(|e: InvalidUri| {
            Status::internal(format!("failed to parse registry URI: {}", e))
        })?;

        let channel = Channel::builder(service_uri)
            .tls_config(service_tls)
            .map_err(|e| Status::internal(format!("failed to create tls config: {}", e)))?
            .connect()
            .await
            .map_err(|e| Status::internal(format!("failed to connect to registry: {}", e)))?;

        let client_archive = ArchiveServiceClient::new(channel);

        let source_digest = build_source(
            request.artifact_context.clone(),
            request.artifact_update,
            client_archive,
            &source,
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

        // Upsert remote source into Vorpal.lock immediately after preparation

        let is_http = artifact_sources
            .last()
            .map(|s| s.path.starts_with("http://") || s.path.starts_with("https://"))
            .unwrap_or(false);

        if is_http {
            let mut lock = match load_lock(&lock_path).await.unwrap_or(None) {
                Some(l) => l,
                None => Lockfile {
                    lockfile: 1,
                    sources: vec![],
                },
            };

            let last = artifact_sources.last().unwrap();
            let mut lockfile_modified = false;

            // Upsert source entry by (name, platform)
            if let Some(existing) = lock
                .sources
                .iter_mut()
                .find(|s| s.name == last.name && s.platform == target_platform)
            {
                let new_digest = last.digest.clone().unwrap_or_default();
                let new_excludes = &last.excludes;
                let new_includes = &last.includes;
                let new_path = &last.path.clone();

                if existing.digest != new_digest
                    || existing.includes != *new_includes
                    || existing.excludes != *new_excludes
                    || existing.path != *new_path
                {
                    existing.digest = new_digest;
                    existing.excludes = new_excludes.clone();
                    existing.includes = new_includes.clone();
                    existing.path = new_path.clone();

                    lockfile_modified = true;
                }
            } else {
                lock.sources.push(LockSource {
                    digest: last.digest.clone().unwrap_or_default(),
                    excludes: last.excludes.clone(),
                    includes: last.includes.clone(),
                    name: last.name.clone(),
                    path: last.path.clone(),
                    platform: target_platform.clone(),
                });

                lockfile_modified = true;
            }

            if lockfile_modified {
                lock.sources
                    .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

                if let Err(e) = save_lock(&lock_path, &lock).await {
                    warn!("Failed to update lockfile {}: {}", lock_path.display(), e);
                } else {
                    info!("Updated lockfile with source: {}", last.name);
                }
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
        artifact: Some(artifact.clone()),
        artifact_digest: Some(artifact_digest.clone()),
        artifact_output: None,
    };

    let _ = tx
        .send(Ok(artifact_response))
        .await
        .map_err(|_| Status::internal("failed to send response"));

    info!(
        "agent |> prepared artifact: {} ({})",
        artifact.name, artifact_digest
    );

    Ok(())
}

#[derive(Debug)]
pub struct AgentServer {
    pub registry: String,
}

impl AgentServer {
    pub fn new(registry: String) -> Self {
        Self { registry }
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

        tokio::spawn(async move {
            if let Err(err) = prepare_artifact(registry, request, &tx).await {
                let _ = tx.send(Err(err)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
