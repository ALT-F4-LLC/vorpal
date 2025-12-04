use crate::command::{
    start::auth,
    store::{
        archives::{compress_zstd, unpack_zstd},
        notary,
        paths::{
            get_artifact_archive_path, get_artifact_output_lock_path, get_artifact_output_path,
            get_file_paths, get_key_ca_path, get_key_service_key_path, set_timestamps,
        },
        temps::{create_sandbox_dir, create_sandbox_file},
    },
};
use anyhow::Result;
use http::uri::Uri;
use sha256::digest;
use std::{fs::Permissions, os::unix::fs::PermissionsExt, path::Path, process::Stdio};
use tokio::{
    fs::{create_dir_all, read, remove_dir_all, remove_file, set_permissions, write},
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{mpsc, mpsc::Sender},
};
use tokio_stream::{
    wrappers::{LinesStream, ReceiverStream},
    StreamExt,
};
use tonic::{
    metadata::{Ascii, MetadataValue},
    transport::{Certificate, Channel, ClientTlsConfig},
    Code::NotFound,
    Request, Response, Status,
};
use tracing::{error, info};
use vorpal_sdk::{
    api::{
        archive::{
            archive_service_client::ArchiveServiceClient, ArchivePullRequest, ArchivePushRequest,
        },
        artifact::{
            artifact_service_client::ArtifactServiceClient, ArtifactSource, ArtifactStep,
            ArtifactSystem, StoreArtifactRequest,
        },
        worker::{
            worker_service_server::WorkerService, BuildArtifactRequest, BuildArtifactResponse,
        },
    },
    artifact::system::get_system_default,
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

#[derive(Debug)]
pub struct WorkerServer {
    pub oauth_issuer: Option<String>,
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
}

impl WorkerServer {
    pub fn new(
        oauth_issuer: Option<String>,
        oauth_client_id: Option<String>,
        oauth_client_secret: Option<String>,
    ) -> Self {
        Self {
            oauth_issuer,
            oauth_client_id,
            oauth_client_secret,
        }
    }
}

/// Obtains OAuth2 service credentials for service-to-service authentication
///
/// Attempts to exchange client credentials for an access token using the OAuth2
/// Client Credentials Flow. Returns None if credentials are not configured.
async fn obtain_service_credentials(
    issuer: Option<&str>,
    client_id: Option<&str>,
    client_secret: Option<&str>,
    scope: &str,
) -> Option<MetadataValue<Ascii>> {
    let issuer = issuer?;
    let client_id = client_id?;
    let client_secret = client_secret?;

    match auth::exchange_client_credentials(issuer, client_id, client_secret, scope).await {
        Ok(token) => {
            info!(
                "worker |> obtained service credentials for scope: {}",
                scope
            );
            Some(token)
        }
        Err(err) => {
            error!(
                "worker |> failed to obtain service credentials for scope {}: {}",
                scope, err
            );
            None
        }
    }
}

/// Helper function to apply authorization header to a request if token is available
fn apply_auth_to_request(
    auth_header: &Option<MetadataValue<Ascii>>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone + '_ {
    move |mut req: Request<()>| {
        if let Some(header) = auth_header {
            req.metadata_mut().insert("authorization", header.clone());
        }

        Ok(req)
    }
}

async fn pull_source(
    archive_auth_header: Option<MetadataValue<Ascii>>,
    artifact_namespace: String,
    artifact_source: &ArtifactSource,
    artifact_source_dir_path: &Path,
    registry: String,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    if artifact_source.digest.is_none() {
        return Err(Status::invalid_argument(
            "artifact source 'digest' is missing",
        ));
    }

    if artifact_source.name.is_empty() {
        return Err(Status::invalid_argument(
            "artifact source 'name' is missing",
        ));
    }

    // Create authenticated archive client
    let ca_pem_path = get_key_ca_path();

    if !ca_pem_path.exists() {
        return Err(Status::internal(format!(
            "CA certificate not found: {}",
            ca_pem_path.display()
        )));
    }

    let service_ca_pem = read(ca_pem_path)
        .await
        .map_err(|e| Status::internal(format!("failed to read CA certificate: {}", e)))?;

    let service_ca = Certificate::from_pem(service_ca_pem);

    let service_tls = ClientTlsConfig::new()
        .ca_certificate(service_ca)
        .domain_name("localhost");

    let client_uri = registry
        .parse::<Uri>()
        .map_err(|e| Status::invalid_argument(format!("invalid registry uri: {e}")))?;

    let client_archive_channel = Channel::builder(client_uri)
        .tls_config(service_tls)
        .map_err(|err| {
            Status::internal(format!("failed to create archive client tls config: {err}"))
        })?
        .connect()
        .await
        .map_err(|err| {
            Status::internal(format!("failed to create archive client channel: {err}"))
        })?;

    // Create client with authorization interceptor if token is available
    let mut client_archive = ArchiveServiceClient::with_interceptor(
        client_archive_channel,
        apply_auth_to_request(&archive_auth_header),
    );

    let source_digest = artifact_source.digest.as_ref().unwrap();
    let source_archive = get_artifact_archive_path(source_digest, &artifact_namespace);

    if !source_archive.exists() {
        send_message(format!("pull source: {source_digest}"), tx).await?;

        let request = ArchivePullRequest {
            digest: source_digest.to_string(),
            namespace: artifact_namespace,
        };

        match client_archive.pull(request).await {
            Err(status) => {
                if status.code() != NotFound {
                    return Err(Status::internal(format!(
                        "failed to pull source archive: {status:?}"
                    )));
                }

                return Err(Status::not_found("source archive not found in registry"));
            }

            Ok(response) => {
                let mut response = response.into_inner();
                let mut response_data = Vec::new();

                while let Ok(message) = response.message().await {
                    if message.is_none() {
                        break;
                    }

                    if let Some(res) = message {
                        if !res.data.is_empty() {
                            response_data.extend(res.data);
                        }
                    }
                }

                if response_data.is_empty() {
                    return Err(Status::not_found("source archive empty in registry"));
                }

                let source_archive_parent = source_archive
                    .parent()
                    .ok_or_else(|| Status::internal("failed to get source archive parent"))?;

                create_dir_all(source_archive_parent)
                    .await
                    .map_err(|err| Status::internal(format!("failed to create source archive parent: {err}")))?;

                write(&source_archive, &response_data)
                    .await
                    .map_err(|err| {
                        Status::internal(format!("failed to write store path: {err}"))
                    })?;

                set_timestamps(&source_archive).await.map_err(|err| {
                    Status::internal(format!("failed to set source timestamps: {err}"))
                })?;
            }
        }
    }

    if !source_archive.exists() {
        return Err(Status::not_found("source archive not found"));
    }

    send_message(format!("unpack source: {source_digest}"), tx).await?;

    let source_workspace_path = artifact_source_dir_path.join(&artifact_source.name);

    if let Err(err) = create_dir_all(&source_workspace_path).await {
        return Err(Status::internal(format!(
            "failed to create source path: {err:?}"
        )));
    }

    if let Err(err) = unpack_zstd(&source_workspace_path, &source_archive).await {
        return Err(Status::internal(format!(
            "failed to unpack source archive: {err:?}"
        )));
    }

    let source_workspace_files = get_file_paths(&source_workspace_path, vec![], vec![])
        .map_err(|err| Status::internal(format!("failed to get source files: {err}")))?;

    for path in source_workspace_files.iter() {
        if let Err(err) = set_timestamps(path).await {
            return Err(Status::internal(format!(
                "failed to sanitize output files: {err:?}"
            )));
        }
    }

    Ok(())
}

fn expand_env(text: &str, envs: &[&String]) -> String {
    envs.iter().fold(text.to_string(), |acc, e| {
        let e = e.split('=').collect::<Vec<&str>>();
        acc.replace(&format!("${}", e[0]), e[1])
    })
}

async fn run_step(
    artifact_digest: &str,
    artifact_namespace: &str,
    artifact_path: &Path,
    step: ArtifactStep,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
    workspace_path: &Path,
) -> Result<(), Status> {
    let mut environments = vec![];

    // Add all artifact environment variables

    let mut paths = vec![];

    for artifact in step.artifacts.iter() {
        let path = get_artifact_output_path(artifact, artifact_namespace);

        if !path.exists() {
            return Err(Status::internal("artifact not found"));
        }

        let path_str = path.display().to_string();

        environments.push(format!("VORPAL_ARTIFACT_{artifact}={path_str}"));

        paths.push(path_str);
    }

    // Add default environment variables

    if !paths.is_empty() {
        environments.push(format!("VORPAL_ARTIFACTS={}", paths.join(" ")))
    }

    environments.extend([
        format!(
            "VORPAL_ARTIFACT_{}={}",
            artifact_digest,
            get_artifact_output_path(artifact_digest, artifact_namespace).display()
        ),
        format!("VORPAL_OUTPUT={}", artifact_path.display()),
        format!("VORPAL_WORKSPACE={}", workspace_path.display()),
    ]);

    // Add all custom environment variables

    environments.extend(step.environments);

    // Add all secrets as environment variables

    let private_key_path = get_key_service_key_path();

    if !private_key_path.exists() {
        return Err(Status::internal("private key not found"));
    }

    for secret in step.secrets.into_iter() {
        let value = notary::decrypt(private_key_path.clone(), secret.value)
            .await
            .map_err(|err| Status::internal(format!("failed to decrypt secret: {err}")))?;

        environments.push(format!("{}={}", secret.name, value));
    }

    // Sort environment variables by key length

    let mut environments_sorted = environments;

    environments_sorted.sort_by_key(|a| a.len());

    let vorpal_envs: Vec<_> = environments_sorted
        .iter()
        .filter(|e| e.starts_with("VORPAL_"))
        .collect();

    // Setup script

    let mut script_path = None;

    if let Some(script) = step.script {
        let script = expand_env(&script, &vorpal_envs);

        let path = workspace_path.join("script.sh");

        write(&path, script)
            .await
            .map_err(|err| Status::internal(format!("failed to write script: {err}")))?;

        set_permissions(&path, Permissions::from_mode(0o755))
            .await
            .map_err(|err| Status::internal(format!("failed to set script permissions: {err}")))?;

        script_path = Some(path);
    }

    // Setup entrypoint

    let entrypoint = step
        .entrypoint
        .or_else(|| script_path.as_ref().map(|path| path.display().to_string()))
        .ok_or_else(|| Status::invalid_argument("entrypoint is missing"))?;

    // Setup command

    let mut command = Command::new(&entrypoint);

    // Setup working directory

    command.current_dir(workspace_path);

    // Setup environment variables

    for env in environments_sorted.iter() {
        let env = env.split('=').collect::<Vec<&str>>();
        let env_value = expand_env(env[1], &vorpal_envs);

        command.env(env[0], env_value);
    }

    // Setup arguments

    if !entrypoint.is_empty() {
        for arg in step.arguments.iter() {
            let arg = expand_env(arg, &vorpal_envs);
            command.arg(arg);
        }

        if let Some(script_path) = script_path {
            command.arg(script_path);
        }
    }

    // Run command

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| Status::internal(format!("failed to spawn sandbox: {err}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Status::internal("Failed to capture stdout from the spawned sandbox"))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Status::internal("Failed to capture stderr from the spawned sandbox"))?;

    let stdout = LinesStream::new(BufReader::new(stdout).lines());
    let stderr = LinesStream::new(BufReader::new(stderr).lines());

    let mut stdio_merged = StreamExt::merge(stdout, stderr);

    let mut last_line = "".to_string();

    while let Some(line) = stdio_merged.next().await {
        let output =
            line.map_err(|err| Status::internal(format!("failed to read sandbox output: {err}")))?;

        last_line = output.clone();

        tx.send(Ok(BuildArtifactResponse { output }))
            .await
            .map_err(|err| Status::internal(format!("failed to send sandbox output: {err}")))?;
    }

    let status = child
        .wait()
        .await
        .map_err(|err| Status::internal(format!("failed to wait for sandbox: {err}")))?;

    if !status.success() {
        return Err(Status::internal(last_line.to_string()));
    }

    Ok(())
}

/// Sends a response to the client and logs errors if any.
async fn send_build_response(
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
    response: Result<BuildArtifactResponse, Status>,
) -> Result<(), Status> {
    tx.send(response).await.map_err(|err| {
        error!("Failed to send response: {:?}", err);
        Status::internal("failed to send response")
    })
}

/// Writes a message to the client stream and propagates errors.
async fn send_message(
    output: String,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    send_build_response(tx, Ok(BuildArtifactResponse { output })).await
}

async fn build_artifact(
    client_id: Option<&str>,
    client_secret: Option<&str>,
    issuer: Option<&str>,
    request: BuildArtifactRequest,
    tx: Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    let artifact = request
        .artifact
        .ok_or_else(|| Status::invalid_argument("artifact is missing"))?;

    let artifact_namespace = &request.artifact_namespace;

    if artifact.name.is_empty() {
        return Err(Status::invalid_argument("artifact 'name' is missing"));
    }

    if artifact.steps.is_empty() {
        return Err(Status::invalid_argument("artifact 'steps' are missing"));
    }

    let artifact_target = ArtifactSystem::try_from(artifact.target).map_err(|err| {
        Status::invalid_argument(format!("artifact failed to parse target: {err}"))
    })?;

    if artifact_target == ArtifactSystem::UnknownSystem {
        return Err(Status::invalid_argument("unknown target"));
    }

    let worker_target = get_system_default()
        .map_err(|err| Status::internal(format!("worker failed to get target: {err}")))?;

    if artifact_target != worker_target {
        return Err(Status::invalid_argument(
            "artifact 'target' unsupported for worker",
        ));
    }

    // Obtain service-to-service OAuth2 tokens for archive and artifact services
    let archive_auth_header =
        obtain_service_credentials(issuer, client_id, client_secret, "archive").await;

    let artifact_auth_header =
        obtain_service_credentials(issuer, client_id, client_secret, "artifact").await;

    // Calculate artifact digest

    let artifact_json = serde_json::to_string(&artifact)
        .map_err(|err| Status::internal(format!("artifact failed to serialize: {err}")))?;

    let artifact_digest = &digest(artifact_json.as_bytes());

    // Check if artifact exists

    let artifact_output_path = get_artifact_output_path(artifact_digest, artifact_namespace);

    if artifact_output_path.exists() {
        error!("worker |> artifact already exists: {}", artifact_digest);
        return Err(Status::already_exists("artifact exists"));
    }

    // Check if artifact is locked

    let artifact_output_lock = get_artifact_output_lock_path(artifact_digest, artifact_namespace);

    if artifact_output_lock.exists() {
        error!("worker |> artifact is locked: {}", artifact_digest);
        return Err(Status::already_exists("artifact is locked"));
    }

    // Create lock file

    let artifact_output_lock_parent = artifact_output_lock
        .parent()
        .ok_or_else(|| Status::internal("failed to get lock file parent"))?;

    create_dir_all(artifact_output_lock_parent)
        .await
        .map_err(|err| Status::internal(format!("failed to create lock file parent: {err}")))?;

    if let Err(err) = write(&artifact_output_lock, artifact_json).await {
        error!("worker |> failed to create lock file: {:?}", err);
        return Err(Status::internal(format!(
            "failed to create lock file: {err:?}"
        )));
    }

    // Create workspace

    let workspace_path = create_sandbox_dir()
        .await
        .map_err(|err| Status::internal(format!("failed to create workspace: {err}")))?;

    let artifact_source_dir_path = workspace_path.join("source");

    if let Err(err) = create_dir_all(&artifact_source_dir_path).await {
        error!("worker |> failed to create source path: {:?}", err);
        return Err(Status::internal(format!(
            "failed to create source path: {err:?}"
        )));
    }

    // Pull sources

    let registry = request.registry;

    for artifact_source in artifact.sources.iter() {
        pull_source(
            archive_auth_header.clone(),
            artifact_namespace.clone(),
            artifact_source,
            &artifact_source_dir_path,
            registry.clone(),
            &tx,
        )
        .await?;

        let source_digest = artifact_source
            .digest
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("source 'digest' is missing"))?;

        info!("worker |> pull source: {}", source_digest);
    }

    // Run steps

    if let Err(err) = create_dir_all(&artifact_output_path).await {
        error!("worker |> failed to create artifact path: {:?}", err);
        return Err(Status::internal(format!(
            "failed to create artifact path: {err:?}"
        )));
    }

    for step in artifact.steps.iter() {
        if let Err(err) = run_step(
            artifact_digest,
            artifact_namespace,
            &artifact_output_path,
            step.clone(),
            &tx,
            &workspace_path,
        )
        .await
        {
            error!("worker |> failed to run step: {:?}", err);
            return Err(Status::internal(err.message()));
        }
    }

    let artifact_path_files = get_file_paths(&artifact_output_path, vec![], vec![])
        .map_err(|err| Status::internal(format!("failed to get output files: {err}")))?;

    if artifact_path_files.len() > 1 {
        send_message(format!("pack: {artifact_digest}"), &tx).await?;

        // Sanitize files

        for path in artifact_path_files.iter() {
            if let Err(err) = set_timestamps(path).await {
                error!("worker |> failed to sanitize output files: {:?}", err);
                return Err(Status::internal(format!(
                    "failed to sanitize output files: {err:?}"
                )));
            }
        }

        // Create archive

        let artifact_archive = create_sandbox_file(Some("tar.zst"))
            .await
            .map_err(|err| Status::internal(format!("failed to create artifact archive: {err}")))?;

        if let Err(err) = compress_zstd(
            &artifact_output_path,
            &artifact_path_files,
            &artifact_archive,
        )
        .await
        {
            error!("worker |> failed to compress artifact: {:?}", err);
            return Err(Status::internal(format!(
                "failed to compress artifact: {err:?}"
            )));
        }

        // TODO: check if archive is already uploaded

        // Upload archive

        // Create authenticated archive client for pushing
        let ca_pem_path = get_key_ca_path();

        if !ca_pem_path.exists() {
            return Err(Status::internal(format!(
                "CA certificate not found: {}",
                ca_pem_path.display()
            )));
        }

        let service_ca_pem = read(ca_pem_path)
            .await
            .map_err(|e| Status::internal(format!("failed to read CA certificate: {}", e)))?;

        let service_ca = Certificate::from_pem(service_ca_pem);

        let service_tls = ClientTlsConfig::new()
            .ca_certificate(service_ca)
            .domain_name("localhost");

        let client_uri = registry
            .parse::<Uri>()
            .map_err(|e| Status::invalid_argument(format!("invalid registry uri: {e}")))?;

        let client_archive_channel = Channel::builder(client_uri.clone())
            .tls_config(service_tls.clone())
            .map_err(|err| {
                Status::internal(format!("failed to create archive client tls config: {err}"))
            })?
            .connect()
            .await
            .map_err(|err| {
                Status::internal(format!("failed to create archive client channel: {err}"))
            })?;

        // Create client with authorization interceptor for pushing if token is available
        let mut client_archive = ArchiveServiceClient::with_interceptor(
            client_archive_channel,
            apply_auth_to_request(&archive_auth_header),
        );

        send_message(format!("push: {artifact_digest}"), &tx).await?;

        let artifact_data = read(&artifact_archive)
            .await
            .map_err(|err| Status::internal(format!("failed to read artifact archive: {err}")))?;

        let mut request_stream = vec![];

        for chunk in artifact_data.chunks(DEFAULT_CHUNKS_SIZE) {
            request_stream.push(ArchivePushRequest {
                data: chunk.to_vec(),
                digest: artifact_digest.clone(),
                namespace: artifact_namespace.clone(),
            });
        }

        if let Err(err) = client_archive
            .push(tokio_stream::iter(request_stream))
            .await
        {
            error!("worker |> failed to push artifact: {:?}", err);
            return Err(Status::internal(format!(
                "failed to push artifact: {err:?}"
            )));
        }

        // Store artifact in registry

        // Create authenticated artifact client
        let ca_pem_path = get_key_ca_path();

        if !ca_pem_path.exists() {
            return Err(Status::internal(format!(
                "CA certificate not found: {}",
                ca_pem_path.display()
            )));
        }

        let service_ca_pem = read(ca_pem_path)
            .await
            .map_err(|e| Status::internal(format!("failed to read CA certificate: {}", e)))?;

        let service_ca = Certificate::from_pem(service_ca_pem);

        let service_tls = ClientTlsConfig::new()
            .ca_certificate(service_ca)
            .domain_name("localhost");

        let client_uri = registry
            .parse::<Uri>()
            .map_err(|e| Status::invalid_argument(format!("invalid registry uri: {e}")))?;

        let client_artifact_channel = Channel::builder(client_uri)
            .tls_config(service_tls)
            .map_err(|err| {
                Status::internal(format!(
                    "failed to create artifact client tls config: {err}"
                ))
            })?
            .connect()
            .await
            .map_err(|err| {
                Status::internal(format!("failed to create artifact client channel: {err}"))
            })?;

        // Create client with authorization interceptor if token is available
        let mut client_artifact = ArtifactServiceClient::with_interceptor(
            client_artifact_channel,
            apply_auth_to_request(&artifact_auth_header),
        );

        let request = StoreArtifactRequest {
            artifact: Some(artifact),
            artifact_aliases: request.artifact_aliases,
            artifact_namespace: request.artifact_namespace,
        };

        client_artifact
            .store_artifact(request)
            .await
            .map_err(|err| {
                Status::internal(format!("failed to store artifact in registry: {err}"))
            })?;

        // Remove artifact archive

        if let Err(err) = remove_file(&artifact_archive).await {
            error!("worker |> failed to remove artifact archive: {:?}", err);
            return Err(Status::internal(format!(
                "failed to remove artifact archive: {err:?}"
            )));
        }
    } else {
        remove_dir_all(&artifact_output_path)
            .await
            .map_err(|err| Status::internal(format!("failed to remove artifact path: {err}")))?;
    }

    // Remove workspace

    if let Err(err) = remove_dir_all(workspace_path).await {
        error!("worker |> failed to remove workspace: {:?}", err);
        return Err(Status::internal(format!(
            "failed to remove workspace: {err:?}"
        )));
    }

    // Remove lock file

    if let Err(err) = remove_file(&artifact_output_lock).await {
        error!("worker |> failed to remove lock file: {:?}", err);
        return Err(Status::internal(format!(
            "failed to remove lock file: {err:?}"
        )));
    }

    info!("worker |> build artifact: {}", artifact_digest);

    Ok(())
}

#[tonic::async_trait]
impl WorkerService for WorkerServer {
    type BuildArtifactStream = ReceiverStream<Result<BuildArtifactResponse, Status>>;

    async fn build_artifact(
        &self,
        request: Request<BuildArtifactRequest>,
    ) -> Result<Response<Self::BuildArtifactStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        let client_id = self.oauth_client_id.clone();
        let client_secret = self.oauth_client_secret.clone();
        let issuer = self.oauth_issuer.clone();

        tokio::spawn(async move {
            if let Err(err) = build_artifact(
                client_id.as_deref(),
                client_secret.as_deref(),
                issuer.as_deref(),
                request.into_inner(),
                tx.clone(),
            )
            .await
            {
                if let Err(err) = send_build_response(&tx, Err(err)).await {
                    error!("Failed to send response: {:?}", err);
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
