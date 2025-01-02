use anyhow::Result;
use sha256::digest;
use std::env::consts::{ARCH, OS};
use std::path::Path;
use std::{fs::Permissions, os::unix::fs::PermissionsExt, process::Stdio};
use tokio::fs::remove_dir_all;
use tokio::fs::{create_dir_all, read, remove_file, write, File};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::{
    fs::set_permissions,
    io::{AsyncBufReadExt, BufReader},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::{Request, Response, Status};
use tracing::error;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactId, ArtifactSourceId, ArtifactStepEnvironment,
};
use vorpal_schema::vorpal::{
    artifact::v0::ArtifactSystem,
    artifact::v0::{
        artifact_service_server::ArtifactService, ArtifactBuildRequest, ArtifactBuildResponse,
    },
};
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::ArtifactSystem::UnknownSystem,
        registry::v0::{
            registry_service_client::RegistryServiceClient, RegistryKind, RegistryPushRequest,
            RegistryRequest,
        },
    },
};
use vorpal_store::temps::{create_sandbox_dir, create_sandbox_file};
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_artifact_lock_path, get_artifact_path, get_cache_path, get_file_paths,
        get_private_key_path, get_source_archive_path, set_timestamps,
    },
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

#[derive(Debug, Default)]
pub struct ArtifactServer {
    pub registry: String,
    pub system: ArtifactSystem,
}

impl ArtifactServer {
    pub fn new(registry: String, system: ArtifactSystem) -> Self {
        Self { registry, system }
    }
}

fn expand_env(text: &str, envs: &[&ArtifactStepEnvironment]) -> String {
    envs.iter().fold(text.to_string(), |acc, e| {
        acc.replace(&format!("${}", e.key), &e.value)
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_step(
    artifact_artifacts: Vec<ArtifactId>,
    artifact_name: String,
    artifact_path: &Path,
    step_arguments: Vec<String>,
    step_entrypoint: Option<String>,
    step_environments: Vec<ArtifactStepEnvironment>,
    step_script: Option<String>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
    workspace_path: &Path,
) -> Result<(), Status> {
    let mut environments = vec![];

    // Add all artifact environment variables

    let mut paths = vec![];

    for artifact in artifact_artifacts.iter() {
        let path = get_artifact_path(&artifact.hash, &artifact.name);

        if !path.exists() {
            return Err(Status::internal("artifact not found"));
        }

        let path_str = path.display().to_string();

        environments.push(ArtifactStepEnvironment {
            key: format!(
                "VORPAL_ARTIFACT_{}",
                artifact.name.to_lowercase().replace('-', "_")
            ),
            value: path_str.clone(),
        });

        paths.push(path_str);
    }

    // Add default environment variables

    let name_envkey = artifact_name.to_lowercase().replace('-', "_");

    environments.extend([
        ArtifactStepEnvironment {
            key: format!("VORPAL_ARTIFACT_{}", name_envkey.clone()),
            value: artifact_path.display().to_string(),
        },
        ArtifactStepEnvironment {
            key: "VORPAL_ARTIFACTS".to_string(),
            value: paths.join(" ").to_string(),
        },
        ArtifactStepEnvironment {
            key: "VORPAL_OUTPUT".to_string(),
            value: artifact_path.display().to_string(),
        },
        ArtifactStepEnvironment {
            key: "VORPAL_WORKSPACE".to_string(),
            value: workspace_path.display().to_string(),
        },
    ]);

    // Add all custom environment variables

    environments.extend(step_environments);

    // Sort environment variables by key length

    let mut environments_sorted = environments;

    environments_sorted.sort_by(|a, b| b.key.len().cmp(&a.key.len()));

    let vorpal_envs: Vec<_> = environments_sorted
        .iter()
        .filter(|e| e.key.starts_with("VORPAL_"))
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
        let env_value = expand_env(&env.value, &vorpal_envs);
        command.env(&env.key, env_value);
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
            if let Err(err) = handle_build(request.into_inner(), registry, tx.clone()).await {
                if let Err(err) = send_build_response(&tx, Err(err)).await {
                    error!("Failed to send response: {:?}", err);
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

async fn handle_build(
    request: ArtifactBuildRequest,
    registry: String,
    tx: Sender<Result<ArtifactBuildResponse, Status>>,
) -> Result<(), Status> {
    let artifact = &request
        .artifact
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("artifact is missing"))?;

    if artifact.name.is_empty() {
        return Err(Status::invalid_argument("name is missing"));
    }

    if artifact.steps.is_empty() {
        return Err(Status::invalid_argument("steps are missing"));
    }

    let manifest_json = serde_json::to_string(&request)
        .map_err(|err| Status::internal(format!("failed to serialize manifest: {:?}", err)))?;

    let request_system = ArtifactSystem::try_from(request.system).unwrap_or(UnknownSystem);

    if request_system == UnknownSystem {
        return Err(Status::invalid_argument("unknown target"));
    }

    let worker_system = format!("{}-{}", ARCH, OS);

    let worker_target = get_artifact_system::<ArtifactSystem>(&worker_system);

    if request_system != worker_target {
        return Err(Status::invalid_argument("target mismatch"));
    }

    let manifest_hash = digest(manifest_json.as_bytes());

    // Check if artifact is locked

    let lock_path = get_artifact_lock_path(&manifest_hash, &artifact.name);

    if lock_path.exists() {
        return Err(Status::already_exists("artifact is locked"));
    }

    // If artifact exists, return

    let artifact_path = get_artifact_path(&manifest_hash, &artifact.name);

    if artifact_path.exists() {
        return Err(Status::already_exists("artifact exists"));
    }

    // Create lock file

    if let Err(err) = write(&lock_path, "").await {
        return Err(Status::internal(format!(
            "failed to create lock file: {:?}",
            err
        )));
    }

    if let Err(err) = create_dir_all(&artifact_path).await {
        return Err(Status::internal(format!(
            "failed to create artifact path: {:?}",
            err
        )));
    }

    // Create workspace

    let workspace_path = create_sandbox_dir()
        .await
        .map_err(|err| Status::internal(format!("failed to create workspace: {:?}", err)))?;

    // let workspace_path_canonical = workspace_path
    //     .canonicalize()
    //     .map_err(|err| Status::internal(format!("failed to canonicalize workspace: {:?}", err)))?;

    // Connect to registry

    let mut registry_client = RegistryServiceClient::connect(registry)
        .await
        .map_err(|err| Status::internal(format!("failed to connect to registry: {:?}", err)))?;

    // Pull any source archives

    pull_source_archives(artifact, &workspace_path, &mut registry_client, &tx).await?;

    // Run artifact steps

    for step in artifact.steps.iter() {
        if let Err(err) = run_step(
            artifact.artifacts.clone(),
            artifact.name.clone(),
            &artifact_path,
            step.arguments.clone(),
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

    // Create artifact tar from build output files

    send_message(&tx, format!("packing: {}", manifest_hash)).await?;

    let artifact_archive_path = create_sandbox_file(Some("tar.zst"))
        .await
        .map_err(|err| Status::internal(format!("failed to create artifact archive: {:?}", err)))?;

    if let Err(err) =
        compress_zstd(&artifact_path, &artifact_path_files, &artifact_archive_path).await
    {
        return Err(Status::internal(format!(
            "failed to compress artifact: {:?}",
            err
        )));
    }

    // upload artifact to registry

    send_message(&tx, format!("pushing: {}", manifest_hash)).await?;

    let artifact_data = read(&artifact_archive_path)
        .await
        .map_err(|err| Status::internal(format!("failed to read artifact archive: {:?}", err)))?;

    let private_key_path = get_private_key_path();

    if !private_key_path.exists() {
        return Err(Status::internal("private key not found"));
    }

    let source_signature = vorpal_notary::sign(private_key_path, &artifact_data)
        .await
        .map_err(|err| Status::internal(format!("failed to sign artifact: {:?}", err)))?;

    let mut request_stream = vec![];

    for chunk in artifact_data.chunks(DEFAULT_CHUNKS_SIZE) {
        request_stream.push(RegistryPushRequest {
            data: chunk.to_vec(),
            data_signature: source_signature.clone().to_vec(),
            hash: manifest_hash.clone(),
            kind: RegistryKind::Artifact as i32,
            name: artifact.name.clone(),
        });
    }

    if let Err(err) = registry_client
        .push(tokio_stream::iter(request_stream))
        .await
    {
        return Err(Status::internal(format!(
            "failed to push artifact: {:?}",
            err
        )));
    }

    // sanitize output files

    for path in artifact_path_files.iter() {
        if let Err(err) = set_timestamps(path).await {
            return Err(Status::internal(format!(
                "failed to sanitize output files: {:?}",
                err
            )));
        }
    }

    // Remove artifact archive

    if let Err(err) = remove_file(&artifact_archive_path).await {
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

    if let Err(err) = remove_file(&lock_path).await {
        return Err(Status::internal(format!(
            "failed to remove lock file: {:?}",
            err
        )));
    }

    Ok(())
}

async fn pull_source_archives(
    artifact: &Artifact,
    workspace_path: &Path,
    registry_client: &mut RegistryServiceClient<tonic::transport::Channel>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
) -> Result<(), Status> {
    let workspace_source_dir_path = workspace_path.join("source");

    if let Err(err) = create_dir_all(&workspace_source_dir_path).await {
        return Err(Status::internal(format!(
            "failed to create source path: {:?}",
            err
        )));
    }

    for source in artifact.sources.iter() {
        handle_source(source, &workspace_source_dir_path, registry_client, tx).await?;
    }

    Ok(())
}

async fn handle_source(
    source: &ArtifactSourceId,
    workspace_source_dir_path: &Path,
    registry_client: &mut RegistryServiceClient<tonic::transport::Channel>,
    tx: &Sender<Result<ArtifactBuildResponse, Status>>,
) -> Result<(), Status> {
    let workspace_source_path = workspace_source_dir_path.join(&source.name);

    if let Err(err) = create_dir_all(&workspace_source_path).await {
        return Err(Status::internal(format!(
            "failed to create source path: {:?}",
            err
        )));
    }

    let source_cache_path = get_cache_path(&source.hash, &source.name);

    if source_cache_path.exists() {
        let source_cache_files = get_file_paths(&source_cache_path, vec![], vec![])
            .map_err(|err| Status::internal(format!("failed to get source files: {:?}", err)))?;

        send_message(
            tx,
            format!("copying source: {}-{}", source.name, source.hash),
        )
        .await?;

        let workspace_source_files = copy_files(
            &source_cache_path,
            source_cache_files.clone(),
            &workspace_source_path,
        )
        .await
        .map_err(|err| Status::internal(format!("failed to copy source files: {:?}", err)))?;

        for path in workspace_source_files.iter() {
            if let Err(err) = set_timestamps(path).await {
                return Err(Status::internal(format!(
                    "failed to sanitize output files: {:?}",
                    err
                )));
            }
        }

        return Ok(());
    }

    let source_archive_path = get_source_archive_path(&source.hash, &source.name);

    if source_archive_path.exists() {
        send_message(
            tx,
            format!("caching source: {}-{}", source.name, source.hash),
        )
        .await?;

        if let Err(err) = create_dir_all(&source_cache_path).await {
            return Err(Status::internal(format!(
                "failed to create source path: {:?}",
                err
            )));
        }

        if let Err(err) = unpack_zstd(&source_cache_path, &source_archive_path).await {
            return Err(Status::internal(format!(
                "failed to unpack source archive: {:?}",
                err
            )));
        }

        let source_cache_files = get_file_paths(&source_cache_path, vec![], vec![])
            .map_err(|err| Status::internal(format!("failed to get source files: {:?}", err)))?;

        send_message(
            tx,
            format!("copying source: {}-{}", source.name, source.hash),
        )
        .await?;

        let workspace_source_files = copy_files(
            &source_cache_path,
            source_cache_files,
            &workspace_source_path,
        )
        .await
        .map_err(|err| Status::internal(format!("failed to copy source files: {:?}", err)))?;

        for path in workspace_source_files.iter() {
            if let Err(err) = set_timestamps(path).await {
                return Err(Status::internal(format!(
                    "failed to sanitize output files: {:?}",
                    err
                )));
            }
        }

        return Ok(());
    }

    send_message(
        tx,
        format!("pulling source: {}-{}", source.name, source.hash),
    )
    .await?;

    let pull_request = RegistryRequest {
        hash: source.hash.clone(),
        name: source.name.clone(),
        kind: RegistryKind::ArtifactSource as i32,
    };

    let response = registry_client.pull(pull_request).await.map_err(|status| {
        Status::internal(format!("failed to pull source archive: {:?}", status))
    })?;

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
        return Ok(());
    }

    let mut source_archive = File::create(&source_archive_path)
        .await
        .map_err(|err| Status::internal(format!("failed to create source archive: {:?}", err)))?;

    if let Err(err) = source_archive.write_all(&response_data).await {
        return Err(Status::internal(format!(
            "failed to write source archive: {:?}",
            err
        )));
    }

    if let Err(err) = set_timestamps(&source_archive_path).await {
        return Err(Status::internal(format!(
            "failed to set source archive timestamps: {:?}",
            err
        )));
    }

    send_message(
        tx,
        format!("caching source: {}-{}", source.name, source.hash),
    )
    .await?;

    if let Err(err) = create_dir_all(&source_cache_path).await {
        return Err(Status::internal(format!(
            "failed to create source path: {:?}",
            err
        )));
    }

    if let Err(err) = unpack_zstd(&source_cache_path, &source_archive_path).await {
        return Err(Status::internal(format!(
            "failed to unpack source archive: {:?}",
            err
        )));
    }

    let source_cache_files = get_file_paths(&source_cache_path, vec![], vec![])
        .map_err(|err| Status::internal(format!("failed to get source files: {:?}", err)))?;

    send_message(
        tx,
        format!("copying source: {}-{}", source.name, source.hash),
    )
    .await?;

    let workspace_source_files = copy_files(
        &source_cache_path,
        source_cache_files.clone(),
        &workspace_source_path,
    )
    .await
    .map_err(|err| Status::internal(format!("failed to copy source files: {:?}", err)))?;

    for path in workspace_source_files.iter() {
        if let Err(err) = set_timestamps(path).await {
            return Err(Status::internal(format!(
                "failed to sanitize output files: {:?}",
                err
            )));
        }
    }

    Ok(())
}
