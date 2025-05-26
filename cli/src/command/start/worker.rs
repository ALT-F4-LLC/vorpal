use crate::command::store::{
    archives::{compress_zstd, unpack_zstd},
    notary,
    paths::{
        get_artifact_archive_path, get_artifact_output_lock_path, get_artifact_output_path,
        get_file_paths, get_key_private_path, set_timestamps,
    },
    temps::{create_sandbox_dir, create_sandbox_file},
};
use anyhow::Result;
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
use tonic::{Code::NotFound, Request, Response, Status};
use tracing::error;
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

#[derive(Debug, Default)]
pub struct WorkerServer {
    pub registry: String,
}

impl WorkerServer {
    pub fn new(registry: String) -> Self {
        Self { registry }
    }
}

async fn pull_source(
    registry: &str,
    source: &ArtifactSource,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
    source_dir_path: &Path,
) -> Result<(), Status> {
    if source.digest.is_none() {
        return Err(Status::invalid_argument(
            "artifact source 'digest' is missing",
        ));
    }

    if source.name.is_empty() {
        return Err(Status::invalid_argument(
            "artifact source 'name' is missing",
        ));
    }

    let source_digest = source.digest.as_ref().unwrap();
    let source_archive = get_artifact_archive_path(source_digest);

    let mut client = ArchiveServiceClient::connect(registry.to_string())
        .await
        .map_err(|err| Status::internal(format!("failed to connect to registry: {:?}", err)))?;

    if !source_archive.exists() {
        send_message(format!("pull source: {}", source_digest), tx).await?;

        let request = ArchivePullRequest {
            digest: source_digest.to_string(),
        };

        match client.pull(request).await {
            Err(status) => {
                if status.code() != NotFound {
                    return Err(Status::internal(format!(
                        "failed to pull source archive: {:?}",
                        status
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

                write(&source_archive, &response_data)
                    .await
                    .map_err(|err| {
                        Status::internal(format!("failed to write store path: {:?}", err))
                    })?;

                set_timestamps(&source_archive).await.map_err(|err| {
                    Status::internal(format!("failed to set source timestamps: {:?}", err))
                })?;
            }
        }
    }

    if !source_archive.exists() {
        return Err(Status::not_found("source archive not found"));
    }

    send_message(format!("unpack source: {}", source_digest), tx).await?;

    let source_workspace_path = source_dir_path.join(&source.name);

    if let Err(err) = create_dir_all(&source_workspace_path).await {
        return Err(Status::internal(format!(
            "failed to create source path: {:?}",
            err
        )));
    }

    if let Err(err) = unpack_zstd(&source_workspace_path, &source_archive).await {
        return Err(Status::internal(format!(
            "failed to unpack source archive: {:?}",
            err
        )));
    }

    let source_workspace_files = get_file_paths(&source_workspace_path, vec![], vec![])
        .map_err(|err| Status::internal(format!("failed to get source files: {:?}", err)))?;

    for path in source_workspace_files.iter() {
        if let Err(err) = set_timestamps(path).await {
            return Err(Status::internal(format!(
                "failed to sanitize output files: {:?}",
                err
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
    artifact_hash: &str,
    artifact_path: &Path,
    step: ArtifactStep,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
    workspace_path: &Path,
) -> Result<(), Status> {
    let mut environments = vec![];

    // Add all artifact environment variables

    let mut paths = vec![];

    for artifact in step.artifacts.iter() {
        let path = get_artifact_output_path(artifact);

        if !path.exists() {
            return Err(Status::internal("artifact not found"));
        }

        let path_str = path.display().to_string();

        environments.push(format!("VORPAL_ARTIFACT_{}={}", artifact, path_str));

        paths.push(path_str);
    }

    // Add default environment variables

    if !paths.is_empty() {
        paths.push(format!("VORPAL_ARTIFACTS={}", paths.join(" ")))
    }

    environments.extend([
        format!(
            "VORPAL_ARTIFACT_{}={}",
            artifact_hash,
            get_artifact_output_path(artifact_hash).display()
        ),
        format!("VORPAL_OUTPUT={}", artifact_path.display()),
        format!("VORPAL_WORKSPACE={}", workspace_path.display()),
    ]);

    // Add all custom environment variables

    environments.extend(step.environments);

    // Add all secrets as environment variables

    let private_key_path = get_key_private_path();

    if !private_key_path.exists() {
        return Err(Status::internal("private key not found"));
    }

    for secret in step.secrets.into_iter() {
        let value = notary::decrypt(private_key_path.clone(), secret.value)
            .await
            .map_err(|err| Status::internal(format!("failed to decrypt secret: {:?}", err)))?;

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
            .map_err(|err| Status::internal(format!("failed to write script: {:?}", err)))?;

        set_permissions(&path, Permissions::from_mode(0o755))
            .await
            .map_err(|err| {
                Status::internal(format!("failed to set script permissions: {:?}", err))
            })?;

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
        .map_err(|err| Status::internal(format!("failed to spawn sandbox: {:?}", err)))?;

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
        let output = line
            .map_err(|err| Status::internal(format!("failed to read sandbox output: {:?}", err)))?;

        last_line = output.clone();

        tx.send(Ok(BuildArtifactResponse { output }))
            .await
            .map_err(|err| Status::internal(format!("failed to send sandbox output: {:?}", err)))?;
    }

    let status = child
        .wait()
        .await
        .map_err(|err| Status::internal(format!("failed to wait for sandbox: {:?}", err)))?;

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
    request: BuildArtifactRequest,
    registry: String,
    tx: Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    let artifact = request
        .artifact
        .ok_or_else(|| Status::invalid_argument("artifact is missing"))?;

    if artifact.name.is_empty() {
        return Err(Status::invalid_argument("artifact 'name' is missing"));
    }

    if artifact.steps.is_empty() {
        return Err(Status::invalid_argument("artifact 'steps' are missing"));
    }

    let artifact_json = serde_json::to_string(&artifact)
        .map_err(|err| Status::internal(format!("artifact failed to serialize: {:?}", err)))?;

    let artifact_target = ArtifactSystem::try_from(artifact.target).map_err(|err| {
        Status::invalid_argument(format!("artifact failed to parse target: {:?}", err))
    })?;

    if artifact_target == ArtifactSystem::UnknownSystem {
        return Err(Status::invalid_argument("unknown target"));
    }

    let worker_target = get_system_default()
        .map_err(|err| Status::internal(format!("worker failed to get target: {:?}", err)))?;

    if artifact_target != worker_target {
        return Err(Status::invalid_argument(
            "artifact 'target' unsupported for worker",
        ));
    }

    let artifact_digest = digest(artifact_json.as_bytes());

    // Check if artifact exists

    let artifact_output_path = get_artifact_output_path(&artifact_digest);

    if artifact_output_path.exists() {
        return Err(Status::already_exists("artifact exists"));
    }

    // Check if artifact is locked

    let artifact_output_lock = get_artifact_output_lock_path(&artifact_digest);

    if artifact_output_lock.exists() {
        return Err(Status::already_exists("artifact is locked"));
    }

    // Create lock file

    if let Err(err) = write(&artifact_output_lock, artifact_json).await {
        return Err(Status::internal(format!(
            "failed to create lock file: {:?}",
            err
        )));
    }

    // Create workspace

    let workspace_path = create_sandbox_dir()
        .await
        .map_err(|err| Status::internal(format!("failed to create workspace: {:?}", err)))?;

    let workspace_source_path = workspace_path.join("source");

    if let Err(err) = create_dir_all(&workspace_source_path).await {
        return Err(Status::internal(format!(
            "failed to create source path: {:?}",
            err
        )));
    }

    // Pull sources

    for source in artifact.sources.iter() {
        pull_source(&registry, source, &tx, &workspace_source_path).await?;
    }

    // Run steps

    if let Err(err) = create_dir_all(&artifact_output_path).await {
        return Err(Status::internal(format!(
            "failed to create artifact path: {:?}",
            err
        )));
    }

    for step in artifact.steps.iter() {
        if let Err(err) = run_step(
            &artifact_digest,
            &artifact_output_path,
            step.clone(),
            &tx,
            &workspace_path,
        )
        .await
        {
            return Err(Status::internal(err.message()));
        }
    }

    let artifact_path_files = get_file_paths(&artifact_output_path, vec![], vec![])
        .map_err(|err| Status::internal(format!("failed to get output files: {:?}", err)))?;

    if artifact_path_files.len() > 1 {
        send_message(format!("pack: {}", artifact_digest), &tx).await?;

        // Sanitize files

        for path in artifact_path_files.iter() {
            if let Err(err) = set_timestamps(path).await {
                return Err(Status::internal(format!(
                    "failed to sanitize output files: {:?}",
                    err
                )));
            }
        }

        // Create archive

        let artifact_archive = create_sandbox_file(Some("tar.zst")).await.map_err(|err| {
            Status::internal(format!("failed to create artifact archive: {:?}", err))
        })?;

        if let Err(err) = compress_zstd(
            &artifact_output_path,
            &artifact_path_files,
            &artifact_archive,
        )
        .await
        {
            return Err(Status::internal(format!(
                "failed to compress artifact: {:?}",
                err
            )));
        }

        // TODO: check if archive is already uploaded

        // Upload archive

        send_message(format!("push: {}", artifact_digest), &tx).await?;

        let artifact_data = read(&artifact_archive).await.map_err(|err| {
            Status::internal(format!("failed to read artifact archive: {:?}", err))
        })?;

        let private_key_path = get_key_private_path();

        if !private_key_path.exists() {
            return Err(Status::internal("private key not found"));
        }

        let artifact_signature = notary::sign(private_key_path, &artifact_data)
            .await
            .map_err(|err| Status::internal(format!("failed to sign artifact: {:?}", err)))?;

        let mut request_stream = vec![];

        for chunk in artifact_data.chunks(DEFAULT_CHUNKS_SIZE) {
            request_stream.push(ArchivePushRequest {
                data: chunk.to_vec(),
                digest: artifact_digest.clone(),
                signature: artifact_signature.clone().to_vec(),
            });
        }

        let mut client = ArchiveServiceClient::connect(registry.clone())
            .await
            .map_err(|err| Status::internal(format!("failed to connect to registry: {:?}", err)))?;

        if let Err(err) = client.push(tokio_stream::iter(request_stream)).await {
            return Err(Status::internal(format!(
                "failed to push artifact: {:?}",
                err
            )));
        }

        // Store artifact in registry

        let mut client = ArtifactServiceClient::connect(registry)
            .await
            .map_err(|err| Status::internal(format!("failed to connect to registry: {:?}", err)))?;

        let request = StoreArtifactRequest {
            artifact: Some(artifact),
            artifact_aliases: request.artifact_aliases,
        };

        client.store_artifact(request).await.map_err(|err| {
            Status::internal(format!("failed to store artifact in registry: {:?}", err))
        })?;

        // Remove artifact archive

        if let Err(err) = remove_file(&artifact_archive).await {
            return Err(Status::internal(format!(
                "failed to remove artifact archive: {:?}",
                err
            )));
        }
    } else {
        remove_dir_all(&artifact_output_path).await.map_err(|err| {
            Status::internal(format!("failed to remove artifact path: {:?}", err))
        })?;
    }

    // Remove workspace

    if let Err(err) = remove_dir_all(workspace_path).await {
        return Err(Status::internal(format!(
            "failed to remove workspace: {:?}",
            err
        )));
    }

    // Remove lock file

    if let Err(err) = remove_file(&artifact_output_lock).await {
        return Err(Status::internal(format!(
            "failed to remove lock file: {:?}",
            err
        )));
    }

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

        let registry = self.registry.clone();

        tokio::spawn(async move {
            if let Err(err) = build_artifact(request.into_inner(), registry, tx.clone()).await {
                if let Err(err) = send_build_response(&tx, Err(err)).await {
                    error!("Failed to send response: {:?}", err);
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
