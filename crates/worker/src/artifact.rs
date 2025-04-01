use anyhow::Result;
use sha256::digest;
use std::collections::HashMap;
use std::path::Path;
use std::{fs::Permissions, os::unix::fs::PermissionsExt, process::Stdio};
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, write};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::{
    fs::set_permissions,
    io::{AsyncBufReadExt, BufReader},
};
use tokio_stream::{
    wrappers::{LinesStream, ReceiverStream},
    StreamExt,
};
use tonic::{Code::NotFound, Request, Response, Status};
use tracing::error;
use vorpal_schema::{
    artifact::v0::{
        artifact_service_server::ArtifactService, ArtifactBuildRequest, ArtifactBuildResponse,
    },
    config::v0::{ConfigArtifactSource, ConfigArtifactSystem},
    registry::v0::{
        registry_service_client::RegistryServiceClient, RegistryArchive, RegistryPullRequest,
        RegistryPushRequest,
    },
    system_default,
};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        get_archive_path, get_file_paths, get_private_key_path, get_store_lock_path,
        get_store_path, set_timestamps,
    },
    temps::{create_sandbox_dir, create_sandbox_file},
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

#[derive(Debug, Default)]
pub struct ArtifactServer {
    pub registry: String,
    pub system: ConfigArtifactSystem,
}

impl ArtifactServer {
    pub fn new(registry: String, system: ConfigArtifactSystem) -> Self {
        Self { registry, system }
    }
}

fn expand_env(text: &str, envs: &[&String]) -> String {
    envs.iter().fold(text.to_string(), |acc, e| {
        let e = e.split('=').collect::<Vec<&str>>();
        acc.replace(&format!("${}", e[0]), e[1])
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_step(
    artifact_hash: &str,
    artifact_path: &Path,
    step_arguments: Vec<String>,
    step_artifacts: Vec<String>,
    step_entrypoint: Option<String>,
    step_environments: Vec<String>,
    step_script: Option<String>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
    workspace_path: &Path,
) -> Result<(), Status> {
    let mut environments = vec![];

    // Add all artifact environment variables

    let mut paths = vec![];

    for artifact in step_artifacts.iter() {
        let path = get_store_path(artifact);

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
            get_store_path(artifact_hash).display()
        ),
        format!("VORPAL_OUTPUT={}", artifact_path.display()),
        format!("VORPAL_WORKSPACE={}", workspace_path.display()),
    ]);

    // Add all custom environment variables

    environments.extend(step_environments);

    // Sort environment variables by key length

    let mut environments_sorted = environments;

    environments_sorted.sort_by(|a, b| a.len().cmp(&b.len()));

    let vorpal_envs: Vec<_> = environments_sorted
        .iter()
        .filter(|e| e.starts_with("VORPAL_"))
        .collect();

    // Setup script

    let mut script_path = None;

    if let Some(script) = step_script {
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

    let entrypoint = step_entrypoint
        .or_else(|| script_path.as_ref().map(|path| path.display().to_string()))
        .ok_or_else(|| Status::invalid_argument("entrypoint is missing"))?;

    // Setup command

    let mut command = Command::new(&entrypoint);

    // Setup working directory

    command.current_dir(workspace_path);

    // Setup environment variables

    for env in environments_sorted.iter() {
        let env = env.split('=').collect::<Vec<&str>>();
        let env_value = expand_env(&env[1], &vorpal_envs);

        command.env(&env[0], env_value);
    }

    // Setup arguments

    if !entrypoint.is_empty() {
        for arg in step_arguments.iter() {
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

    while let Some(line) = stdio_merged.next().await {
        let output = line
            .map_err(|err| Status::internal(format!("failed to read sandbox output: {:?}", err)))?;

        tx.send(Ok(ArtifactBuildResponse { output }))
            .await
            .map_err(|err| Status::internal(format!("failed to send sandbox output: {:?}", err)))?;
    }

    let status = child
        .wait()
        .await
        .map_err(|err| Status::internal(format!("failed to wait for sandbox: {:?}", err)))?;

    if !status.success() {
        return Err(Status::internal("sandbox failed"));
    }

    Ok(())
}

/// Sends a response to the client and logs errors if any.
async fn send_build_response(
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
    response: Result<ArtifactBuildResponse, Status>,
) -> Result<(), Status> {
    tx.send(response).await.map_err(|err| {
        error!("Failed to send response: {:?}", err);
        Status::internal("failed to send response")
    })
}

/// Writes a message to the client stream and propagates errors.
async fn send_message(
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
    message: String,
) -> Result<(), Status> {
    send_build_response(tx, Ok(ArtifactBuildResponse { output: message })).await
}

#[tonic::async_trait]
impl ArtifactService for ArtifactServer {
    type BuildStream = ReceiverStream<Result<ArtifactBuildResponse, Status>>;

    async fn build(
        &self,
        request: Request<ArtifactBuildRequest>,
    ) -> Result<Response<Self::BuildStream>, Status> {
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

async fn build_artifact(
    request: ArtifactBuildRequest,
    registry: String,
    tx: Sender<Result<ArtifactBuildResponse, Status>>,
) -> Result<(), Status> {
    if request.artifact.is_none() {
        return Err(Status::invalid_argument("build 'artifact' is missing"));
    }

    let artifact = request.artifact.unwrap();
    let artifact_source_hash = request.artifact_source_hash;

    if artifact.name.is_empty() {
        return Err(Status::invalid_argument("artifact 'name' is missing"));
    }

    if artifact.steps.is_empty() {
        return Err(Status::invalid_argument("artifact 'steps' are missing"));
    }

    let artifact_json = serde_json::to_string(&artifact)
        .map_err(|err| Status::internal(format!("artifact failed to serialize: {:?}", err)))?;

    let artifact_target = ConfigArtifactSystem::try_from(artifact.target).map_err(|err| {
        Status::invalid_argument(format!("artifact failed to parse target: {:?}", err))
    })?;

    if artifact_target == ConfigArtifactSystem::UnknownSystem {
        return Err(Status::invalid_argument("unknown target"));
    }

    let worker_target = system_default()
        .map_err(|err| Status::internal(format!("worker failed to get target: {:?}", err)))?;

    if artifact_target != worker_target {
        return Err(Status::invalid_argument(
            "artifact 'target' unsupported for worker",
        ));
    }

    let artifact_hash = digest(artifact_json.as_bytes());

    // Check if artifact exists

    let artifact_path = get_store_path(&artifact_hash);

    if artifact_path.exists() {
        return Err(Status::already_exists("artifact exists"));
    }

    // Check if artifact is locked

    let artifact_lock = get_store_lock_path(&artifact_hash);

    if artifact_lock.exists() {
        return Err(Status::already_exists("artifact is locked"));
    }

    // Create lock file

    if let Err(err) = write(&artifact_lock, artifact_json).await {
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

    let mut client_registry = RegistryServiceClient::connect(registry)
        .await
        .map_err(|err| Status::internal(format!("failed to connect to registry: {:?}", err)))?;

    for artifact_source in artifact.sources.iter() {
        pull_source(
            artifact_source,
            &artifact_source_hash,
            &mut client_registry,
            &tx,
            &workspace_source_path,
        )
        .await?;
    }

    // Run steps

    if let Err(err) = create_dir_all(&artifact_path).await {
        return Err(Status::internal(format!(
            "failed to create artifact path: {:?}",
            err
        )));
    }

    for step in artifact.steps.iter() {
        if let Err(err) = run_step(
            &artifact_hash,
            &artifact_path,
            step.arguments.clone(),
            step.artifacts.clone(),
            step.entrypoint.clone(),
            step.environments.clone(),
            step.script.clone(),
            &tx,
            &workspace_path,
        )
        .await
        {
            return Err(Status::internal(format!("failed to run step: {:?}", err)));
        }
    }

    let artifact_path_files = get_file_paths(&artifact_path, vec![], vec![])
        .map_err(|err| Status::internal(format!("failed to get output files: {:?}", err)))?;

    if artifact_path_files.is_empty() || artifact_path_files.len() == 1 {
        return Err(Status::internal("no output files found"));
    }

    // Sanitize files

    send_message(&tx, format!("sanitize: {}", artifact_hash)).await?;

    for path in artifact_path_files.iter() {
        if let Err(err) = set_timestamps(path).await {
            return Err(Status::internal(format!(
                "failed to sanitize output files: {:?}",
                err
            )));
        }
    }

    // Create archive

    send_message(&tx, format!("pack: {}", artifact_hash)).await?;

    let artifact_archive = create_sandbox_file(Some("tar.zst"))
        .await
        .map_err(|err| Status::internal(format!("failed to create artifact archive: {:?}", err)))?;

    if let Err(err) = compress_zstd(&artifact_path, &artifact_path_files, &artifact_archive).await {
        return Err(Status::internal(format!(
            "failed to compress artifact: {:?}",
            err
        )));
    }

    // TODO: check if archive is already uploaded

    // Upload archive

    send_message(&tx, format!("push: {}", artifact_hash)).await?;

    let artifact_data = read(&artifact_archive)
        .await
        .map_err(|err| Status::internal(format!("failed to read artifact archive: {:?}", err)))?;

    let private_key_path = get_private_key_path();

    if !private_key_path.exists() {
        return Err(Status::internal("private key not found"));
    }

    let artifact_signature = vorpal_notary::sign(private_key_path, &artifact_data)
        .await
        .map_err(|err| Status::internal(format!("failed to sign artifact: {:?}", err)))?;

    let mut request_stream = vec![];

    for chunk in artifact_data.chunks(DEFAULT_CHUNKS_SIZE) {
        request_stream.push(RegistryPushRequest {
            archive: RegistryArchive::Artifact as i32,
            data: chunk.to_vec(),
            hash: artifact_hash.clone(),
            signature: artifact_signature.clone().to_vec(),
        });
    }

    if let Err(err) = client_registry
        .push_archive(tokio_stream::iter(request_stream))
        .await
    {
        return Err(Status::internal(format!(
            "failed to push artifact: {:?}",
            err
        )));
    }

    // TODO: put artifact in registry

    // Remove artifact archive

    if let Err(err) = remove_file(&artifact_archive).await {
        return Err(Status::internal(format!(
            "failed to remove artifact archive: {:?}",
            err
        )));
    }

    // Remove workspace

    if let Err(err) = remove_dir_all(workspace_path).await {
        return Err(Status::internal(format!(
            "failed to remove workspace: {:?}",
            err
        )));
    }

    // Remove lock file

    if let Err(err) = remove_file(&artifact_lock).await {
        return Err(Status::internal(format!(
            "failed to remove lock file: {:?}",
            err
        )));
    }

    Ok(())
}

async fn pull_source(
    artifact_source: &ConfigArtifactSource,
    artifact_source_hash: &HashMap<String, String>,
    client_registry: &mut RegistryServiceClient<tonic::transport::Channel>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
    workspace_source_dir_path: &Path,
) -> Result<(), Status> {
    let source_name = artifact_source.name.clone();

    if source_name.is_empty() {
        return Err(Status::invalid_argument("source 'name' is missing"));
    }

    let source_hash = artifact_source_hash.get(&source_name).ok_or_else(|| {
        Status::invalid_argument(format!("source 'hash' not found: {}", artifact_source.name))
    })?;

    let source_archive = get_archive_path(&source_hash);

    if !source_archive.exists() {
        send_message(
            tx,
            format!("pull source: {}-{}", artifact_source.name, &source_hash),
        )
        .await?;

        let registry_request = RegistryPullRequest {
            archive: RegistryArchive::ArtifactSource as i32,
            hash: source_hash.clone(),
        };

        match client_registry.pull_archive(registry_request).await {
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

    send_message(tx, format!("unpack source: {}", artifact_source.name)).await?;

    let source_workspace_path = workspace_source_dir_path.join(&artifact_source.name);

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

    send_message(tx, format!("sanitize source: {}", artifact_source.name)).await?;

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
