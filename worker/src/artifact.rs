use anyhow::Result;
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
use tonic::{Code::NotFound, Request, Response, Status};
use tracing::error;
use vorpal_schema::vorpal::artifact::v0::{ArtifactEnvironment, ArtifactId};
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
            registry_service_client::RegistryServiceClient, RegistryPullRequest,
            RegistryPushRequest, RegistryStoreKind,
        },
    },
};
use vorpal_store::temps::create_sandbox_dir;
use vorpal_store::{
    archives::{compress_zstd, unpack_zstd},
    paths::{
        copy_files, get_artifact_archive_path, get_artifact_lock_path, get_artifact_path,
        get_file_paths, get_private_key_path, get_source_archive_path, get_source_path,
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

#[allow(clippy::too_many_arguments)]
async fn run_step(
    artifact_artifacts: Vec<ArtifactId>,
    artifact_name: String,
    artifact_path: &Path,
    step_arguments: Vec<String>,
    step_entrypoint: Option<String>,
    step_environments: Vec<ArtifactEnvironment>,
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

        environments.push(ArtifactEnvironment {
            key: format!(
                "VORPAL_ARTIFACT_{}",
                artifact.name.to_lowercase().replace('-', "_")
            ),
            value: path.display().to_string(),
        });

        paths.push(path.display().to_string());
    }

    // Add default environment variables

    let name_envkey = artifact_name.to_lowercase().replace('-', "_");

    environments.push(ArtifactEnvironment {
        key: format!("VORPAL_ARTIFACT_{}", name_envkey.clone()),
        value: artifact_path.display().to_string(),
    });

    environments.push(ArtifactEnvironment {
        key: "VORPAL_ARTIFACTS".to_string(),
        value: paths.join(" ").to_string(),
    });

    environments.push(ArtifactEnvironment {
        key: "VORPAL_OUTPUT".to_string(),
        value: artifact_path.display().to_string(),
    });

    environments.push(ArtifactEnvironment {
        key: "VORPAL_WORKSPACE".to_string(),
        value: workspace_path.display().to_string(),
    });

    // Add all custom environment variables

    for environment in step_environments.clone() {
        environments.push(environment);
    }

    // Sort environment variables by key length

    let mut environments_sorted = environments.to_vec();

    environments_sorted.sort_by(|a, b| b.key.len().cmp(&a.key.len()));

    // Setup script

    let mut script_path = None;

    if let Some(script) = &step_script {
        let mut script = script.clone();

        for env in environments_sorted.clone() {
            script = script.replace(&format!("${}", env.key), &env.value);
        }

        let path = workspace_path.join("script.sh");

        write(&path, script.clone())
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

    let entrypoint = match step_entrypoint.clone() {
        Some(entrypoint) => entrypoint,
        None => match script_path {
            Some(ref path) => path.display().to_string(),
            None => return Err(Status::invalid_argument("entrypoint is missing")),
        },
    };

    // Setup command

    let mut command = Command::new(entrypoint.clone());

    // Setup working directory

    command.current_dir(workspace_path);

    // Setup environment variables

    for env in environments_sorted.clone() {
        let mut value = env.value.clone();

        for e in environments_sorted.clone() {
            value = value.replace(&format!("${}", e.key), &e.value);
        }

        command.env(env.key, value);
    }

    // Setup arguments

    if !entrypoint.is_empty() {
        for arg in step_arguments.iter() {
            let mut arg = arg.clone();

            for env in environments_sorted.clone() {
                arg = arg.replace(&format!("${}", env.key), &env.value);
            }

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

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

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
            let request = request.into_inner();

            if request.name.is_empty() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("name is missing")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            if request.hash.is_empty() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("hash is missing")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            if request.steps.is_empty() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("steps are missing")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            let request_target = match ArtifactSystem::try_from(request.target) {
                Ok(target) => target,
                Err(_) => UnknownSystem,
            };

            if request_target == UnknownSystem {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("unknown target")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            let worker_system = format!("{}-{}", ARCH, OS);

            let worker_target = get_artifact_system::<ArtifactSystem>(worker_system.as_str());

            if request_target != worker_target {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("target mismatch")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // Check if artifact is locked

            let artifact_lock_path = get_artifact_lock_path(&request.hash, &request.name);

            if artifact_lock_path.exists() {
                if let Err(err) = tx
                    .send(Err(Status::already_exists("artifact is locked")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // If artifact exists, return

            let artifact_path = get_artifact_path(&request.hash, &request.name);

            if artifact_path.exists() {
                if let Err(err) = tx
                    .send(Err(Status::already_exists("artifact exists")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // Create lock file

            if let Err(err) = write(&artifact_lock_path, "").await {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to create lock file: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            if let Err(err) = create_dir_all(&artifact_path).await {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to create artifact path: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // Create workspace

            let workspace_path = match create_sandbox_dir().await {
                Ok(path) => path,
                Err(err) => {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to create workspace: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            };

            let workspace_path_canonical = match workspace_path.canonicalize() {
                Ok(path) => path,
                Err(err) => {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to canonicalize workspace: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            };

            let workspace_source_path = workspace_path_canonical.join("source");

            if let Err(err) = create_dir_all(&workspace_source_path).await {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to create workspace source path: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // Connect to registry

            let mut registry_client = match RegistryServiceClient::connect(registry).await {
                Ok(client) => client,
                Err(err) => {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to connect to registry: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            };

            // Create new artifact id

            let id = ArtifactId {
                hash: request.hash.clone(),
                name: request.name.clone(),
            };

            // determine if we need to download source archive

            let source_path = get_source_path(&request.hash, &request.name);

            if !source_path.exists() {
                let pull_request = RegistryPullRequest {
                    artifact_id: Some(id.clone()),
                    kind: RegistryStoreKind::ArtifactSource as i32,
                };

                match registry_client.pull(pull_request.clone()).await {
                    Ok(response) => {
                        let mut response = response.into_inner();
                        let mut response_data = Vec::new();

                        while let Ok(message) = response.message().await {
                            if message.is_none() {
                                break;
                            }

                            if let Some(res) = message {
                                if !res.data.is_empty() {
                                    response_data.extend_from_slice(&res.data);
                                }
                            }
                        }

                        if !response_data.is_empty() {
                            let source_archive_path = get_source_archive_path(&id.hash, &id.name);

                            let mut source_archive = match File::create(&source_archive_path).await
                            {
                                Ok(file) => file,
                                Err(err) => {
                                    if let Err(err) = tx
                                        .send(Err(Status::internal(format!(
                                            "failed to create source archive: {:?}",
                                            err
                                        ))))
                                        .await
                                    {
                                        error!("failed to send error: {:?}", err);
                                    }

                                    return;
                                }
                            };

                            if let Err(err) = source_archive.write_all(&response_data).await {
                                if let Err(err) = tx
                                    .send(Err(Status::internal(format!(
                                        "failed to write source archive: {:?}",
                                        err
                                    ))))
                                    .await
                                {
                                    error!("failed to send error: {:?}", err);
                                }
                            }

                            if let Err(err) = create_dir_all(&source_path).await {
                                if let Err(err) = tx
                                    .send(Err(Status::internal(format!(
                                        "failed to create source path: {:?}",
                                        err
                                    ))))
                                    .await
                                {
                                    error!("failed to send error: {:?}", err);
                                }

                                return;
                            }

                            if let Err(err) = unpack_zstd(&source_path, &source_archive_path).await
                            {
                                if let Err(err) = tx
                                    .send(Err(Status::internal(format!(
                                        "failed to unpack source archive: {:?}",
                                        err
                                    ))))
                                    .await
                                {
                                    error!("failed to send error: {:?}", err);
                                }

                                return;
                            }

                            if let Err(err) = remove_file(&source_archive_path).await {
                                if let Err(err) = tx
                                    .send(Err(Status::internal(format!(
                                        "failed to remove source archive: {:?}",
                                        err
                                    ))))
                                    .await
                                {
                                    error!("failed to send error: {:?}", err);
                                }

                                return;
                            }
                        }
                    }

                    Err(status) => {
                        if status.code() != NotFound {
                            if let Err(err) = tx
                                .send(Err(Status::internal(format!(
                                    "failed to pull source archive: {:?}",
                                    status
                                ))))
                                .await
                            {
                                error!("failed to send error: {:?}", err);
                            }

                            return;
                        }
                    }
                }
            }

            if source_path.exists() {
                let source_files = match get_file_paths(&source_path, vec![], vec![]) {
                    Ok(files) => files,
                    Err(err) => {
                        if let Err(err) = tx
                            .send(Err(Status::internal(format!(
                                "failed to get source files: {:?}",
                                err
                            ))))
                            .await
                        {
                            error!("failed to send error: {:?}", err);
                        }

                        return;
                    }
                };

                if let Err(err) =
                    copy_files(&source_path, source_files, &workspace_source_path).await
                {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to copy source files: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            }

            // Run artifact steps

            for step in request.steps.iter() {
                if let Err(err) = run_step(
                    request.artifacts.clone(),
                    request.name.clone(),
                    &artifact_path,
                    step.arguments.clone(),
                    step.entrypoint.clone(),
                    step.environments.clone(),
                    step.script.clone(),
                    &tx,
                    &workspace_path_canonical,
                )
                .await
                {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to run step: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            }

            // Check for output files

            let artifact_path_files = match get_file_paths(&artifact_path, vec![], vec![]) {
                Ok(files) => files,
                Err(err) => {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to get output files: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            };

            if artifact_path_files.is_empty() || artifact_path_files.len() == 1 {
                if let Err(err) = tx
                    .send(Err(Status::internal("no output files found")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // Create artifact tar from build output files

            let artifact_archive_path = get_artifact_archive_path(&request.hash, &request.name);

            if let Err(err) =
                compress_zstd(&artifact_path, &artifact_path_files, &artifact_archive_path).await
            {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to compress artifact: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // uplaod artifact to registry

            let artifact_data = match read(&artifact_archive_path).await {
                Ok(data) => data,
                Err(err) => {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to read artifact archive: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            };

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                if let Err(err) = tx
                    .send(Err(Status::internal("private key not found")))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            let source_signature = match vorpal_notary::sign(private_key_path, &artifact_data).await
            {
                Ok(signature) => signature,
                Err(err) => {
                    if let Err(err) = tx
                        .send(Err(Status::internal(format!(
                            "failed to sign artifact: {:?}",
                            err
                        ))))
                        .await
                    {
                        error!("failed to send error: {:?}", err);
                    }

                    return;
                }
            };

            let mut request_stream = vec![];

            let artifact_id = ArtifactId {
                hash: request.hash.clone(),
                name: request.name.clone(),
            };

            for chunk in artifact_data.chunks(DEFAULT_CHUNKS_SIZE) {
                request_stream.push(RegistryPushRequest {
                    artifact_id: Some(artifact_id.clone()),
                    data: chunk.to_vec(),
                    data_signature: source_signature.clone().to_vec(),
                    kind: RegistryStoreKind::Artifact as i32,
                });
            }

            if let Err(err) = registry_client
                .push(tokio_stream::iter(request_stream))
                .await
            {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to push artifact: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            if let Err(err) = remove_file(&artifact_archive_path).await {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to remove artifact archive: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            // Remove lock file

            if let Err(err) = remove_file(&artifact_lock_path).await {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to remove lock file: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }

                return;
            }

            if let Err(err) = remove_dir_all(workspace_path).await {
                if let Err(err) = tx
                    .send(Err(Status::internal(format!(
                        "failed to remove workspace: {:?}",
                        err
                    ))))
                    .await
                {
                    error!("failed to send error: {:?}", err);
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
