use crate::command::{
    start::{auth, secrets},
    store::{
        archives::{compress_zstd, unpack_zstd},
        notary,
        paths::{
            get_artifact_archive_path, get_artifact_output_lock_path, get_artifact_output_path,
            get_file_paths, get_key_service_key_path, get_root_secret_dir_path, set_timestamps,
        },
        temps::{create_private_dir, create_private_file, create_sandbox_dir, create_sandbox_file},
    },
};
use anyhow::Result;
use fs4::fs_std::FileExt;
use sha256::digest;
use std::{
    collections::HashSet,
    fs::Permissions,
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    process::Stdio,
};
use tokio::{
    fs::{create_dir_all, remove_dir_all, remove_file, set_permissions, write},
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    sync::{mpsc, mpsc::Sender},
};
use tokio_stream::{
    wrappers::{LinesStream, ReceiverStream},
    StreamExt,
};
use tonic::{
    metadata::{Ascii, MetadataValue},
    Code::NotFound,
    Request, Response, Status,
};
use tracing::{error, info};
use uuid::Uuid;
use vorpal_sdk::{
    api::{
        archive::{
            archive_service_client::ArchiveServiceClient, ArchivePullRequest, ArchivePushRequest,
        },
        artifact::{
            artifact_service_client::ArtifactServiceClient, Artifact, ArtifactSource, ArtifactStep,
            ArtifactStepSecret, ArtifactSystem, StoreArtifactRequest,
        },
        worker::{
            worker_service_server::WorkerService, BuildArtifactRequest, BuildArtifactResponse,
        },
    },
    artifact::system::get_system_default,
    context::build_channel,
};

const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

/// The variable naming a step's secret directory, which holds one file per secret.
const SECRET_DIR_ENV: &str = "VORPAL_SECRET_DIR";

/// The bwrap options that mount or create something at a sandbox path, each with its
/// argument count. The destination is the last argument.
const BWRAP_DESTINATION_OPTIONS: [(&str, usize); 21] = [
    ("--bind", 2),
    ("--bind-try", 2),
    ("--dev-bind", 2),
    ("--dev-bind-try", 2),
    ("--ro-bind", 2),
    ("--ro-bind-try", 2),
    ("--bind-fd", 2),
    ("--ro-bind-fd", 2),
    ("--file", 2),
    ("--bind-data", 2),
    ("--ro-bind-data", 2),
    ("--symlink", 2),
    ("--overlay", 3),
    ("--remount-ro", 1),
    ("--tmp-overlay", 1),
    ("--ro-overlay", 1),
    ("--proc", 1),
    ("--dev", 1),
    ("--tmpfs", 1),
    ("--mqueue", 1),
    ("--dir", 1),
];

#[derive(Debug)]
pub struct WorkerServer {
    pub issuer_audience: Option<String>,
    pub issuer_client_id: Option<String>,
    pub issuer_client_secret: Option<String>,
    pub issuer: Option<String>,
}

impl WorkerServer {
    pub fn new(
        issuer: Option<String>,
        issuer_audience: Option<String>,
        issuer_client_id: Option<String>,
        issuer_client_secret: Option<String>,
    ) -> Self {
        if let Err(err) = remove_orphaned_secret_dirs(&get_root_secret_dir_path()) {
            error!("worker |> failed to remove orphaned secret directories: {err}");
        }

        Self {
            issuer_audience,
            issuer_client_id,
            issuer_client_secret,
            issuer,
        }
    }
}

/// A step's secret directory, holding one owner-only file per secret. It stays locked
/// while held, so start-up cleanup in another worker instance leaves it alone, and it is
/// removed when dropped, on every path out of the step.
#[derive(Debug)]
struct SecretDir {
    path: PathBuf,
    _lock: std::fs::File,
}

impl SecretDir {
    /// Creates `path` and locks it, creating its parent owner-only when missing.
    fn create(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            match create_private_dir(parent) {
                Err(err) if err.kind() != io::ErrorKind::AlreadyExists => return Err(err),
                _ => {}
            }
        }

        create_private_dir(&path)?;

        let lock = std::fs::File::open(&path).and_then(|lock| {
            if lock.try_lock_exclusive()? {
                Ok(lock)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "secret directory is locked by another worker",
                ))
            }
        });

        match lock {
            Ok(lock) => Ok(Self { path, _lock: lock }),
            Err(err) => {
                if let Err(remove_err) = std::fs::remove_dir(&path) {
                    error!(
                        "worker |> failed to remove secret directory {}: {remove_err}",
                        path.display()
                    );
                }
                Err(err)
            }
        }
    }

    /// Writes `value` to the file `name`, which must be a validated secret name.
    fn write(&self, name: &str, value: &[u8]) -> io::Result<()> {
        create_private_file(&self.path.join(name))?.write_all(value)
    }
}

impl Drop for SecretDir {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_dir_all(&self.path) {
            error!(
                "worker |> failed to remove secret directory {}: {err}",
                self.path.display()
            );
        }
    }
}

/// Removes every secret directory under `secret_parent_path` that no running worker
/// holds: those a worker left behind when it stopped without dropping them.
fn remove_orphaned_secret_dirs(secret_parent_path: &Path) -> io::Result<()> {
    let entries = match std::fs::read_dir(secret_parent_path) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    for entry in entries {
        let entry = entry?;

        if !entry.file_type()?.is_dir() {
            continue;
        }

        let path = entry.path();
        let lock = std::fs::File::open(&path)?;

        if lock.try_lock_exclusive()? {
            std::fs::remove_dir_all(&path)?;
        }
    }

    Ok(())
}

/// Obtains `OAuth2` service credentials for service-to-service authentication
///
/// Attempts to exchange client credentials for an access token using the `OAuth2`
/// Client Credentials Flow. Returns None if credentials are not configured.
async fn obtain_service_credentials(
    issuer: Option<&str>,
    issuer_audience: Option<&str>,
    issuer_client_id: Option<&str>,
    issuer_client_secret: Option<&str>,
    issuer_scope: &str,
) -> Option<(MetadataValue<Ascii>, u64)> {
    let issuer = issuer?;
    let issuer_client_id = issuer_client_id?;
    let issuer_client_secret = issuer_client_secret?;

    match auth::exchange_client_credentials(
        issuer,
        issuer_audience,
        issuer_client_id,
        issuer_client_secret,
        issuer_scope,
    )
    .await
    {
        Ok((token, expires_in)) => {
            info!(
                "worker |> obtained service credentials for scope: {} (expires in {}s)",
                issuer_scope, expires_in
            );
            Some((token, expires_in))
        }
        Err(err) => {
            error!(
                "worker |> failed to obtain service credentials for scope {}: {}",
                issuer_scope, err
            );
            None
        }
    }
}

/// Helper function to apply authorization header to a request if token is available
fn apply_auth_to_request(
    auth_header: Option<&MetadataValue<Ascii>>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone + '_ {
    move |mut req: Request<()>| {
        // Fn interceptor closure: may be invoked more than once per client,
        // each call needs its own owned header without consuming the capture.
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
    if artifact_source.name.is_empty() {
        return Err(Status::invalid_argument(
            "artifact source 'name' is missing",
        ));
    }

    // Create authenticated archive client
    let client_archive_channel = build_channel(&registry)
        .await
        .map_err(|e| Status::internal(format!("failed to connect to registry: {e}")))?;

    // Create client with authorization interceptor if token is available
    let mut client_archive = ArchiveServiceClient::with_interceptor(
        client_archive_channel,
        apply_auth_to_request(archive_auth_header.as_ref()),
    );

    let Some(source_digest) = artifact_source.digest.as_ref() else {
        return Err(Status::invalid_argument(
            "artifact source 'digest' is missing",
        ));
    };
    let source_archive = get_artifact_archive_path(source_digest, &artifact_namespace);

    if !source_archive.exists() {
        send_message(format!("pull source: {source_digest}"), tx).await?;

        // source_digest is still used below (unpack log message)
        let request = ArchivePullRequest {
            digest: source_digest.clone(),
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

                create_dir_all(source_archive_parent).await.map_err(|err| {
                    Status::internal(format!("failed to create source archive parent: {err}"))
                })?;

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

    for path in &source_workspace_files {
        if let Err(err) = set_timestamps(path).await {
            return Err(Status::internal(format!(
                "failed to sanitize output files: {err:?}"
            )));
        }
    }

    Ok(())
}

async fn pull_artifact(
    archive_auth_header: Option<&MetadataValue<Ascii>>,
    artifact_namespace: &str,
    artifact_digest: &str,
    registry: &str,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    let artifact_output_path = get_artifact_output_path(artifact_digest, artifact_namespace);

    if artifact_output_path.exists() {
        return Ok(());
    }

    let artifact_archive_path = get_artifact_archive_path(artifact_digest, artifact_namespace);

    if !artifact_archive_path.exists() {
        send_message(format!("pull artifact: {artifact_digest}"), tx).await?;

        let client_archive_channel = build_channel(registry)
            .await
            .map_err(|e| Status::internal(format!("failed to connect to registry: {e}")))?;

        let mut client_archive = ArchiveServiceClient::with_interceptor(
            client_archive_channel,
            apply_auth_to_request(archive_auth_header),
        );

        let request = ArchivePullRequest {
            digest: artifact_digest.to_string(),
            namespace: artifact_namespace.to_string(),
        };

        match client_archive.pull(request).await {
            Err(status) => {
                if status.code() != NotFound {
                    return Err(Status::internal(format!(
                        "failed to pull artifact archive: {status:?}"
                    )));
                }

                return Err(Status::not_found("artifact archive not found in registry"));
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
                    return Err(Status::not_found("artifact archive empty in registry"));
                }

                let archive_parent = artifact_archive_path
                    .parent()
                    .ok_or_else(|| Status::internal("failed to get artifact archive parent"))?;

                create_dir_all(archive_parent).await.map_err(|err| {
                    Status::internal(format!("failed to create artifact archive parent: {err}"))
                })?;

                write(&artifact_archive_path, &response_data)
                    .await
                    .map_err(|err| {
                        Status::internal(format!("failed to write artifact archive: {err}"))
                    })?;

                set_timestamps(&artifact_archive_path)
                    .await
                    .map_err(|err| {
                        Status::internal(format!(
                            "failed to set artifact archive timestamps: {err}"
                        ))
                    })?;
            }
        }
    }

    if !artifact_archive_path.exists() {
        return Err(Status::not_found("artifact archive not found"));
    }

    send_message(format!("unpack artifact: {artifact_digest}"), tx).await?;

    create_dir_all(&artifact_output_path)
        .await
        .map_err(|err| Status::internal(format!("failed to create artifact output path: {err}")))?;

    unpack_zstd(&artifact_output_path, &artifact_archive_path)
        .await
        .map_err(|err| Status::internal(format!("failed to unpack artifact archive: {err:?}")))?;

    let artifact_files = get_file_paths(&artifact_output_path, vec![], vec![])
        .map_err(|err| Status::internal(format!("failed to get artifact files: {err}")))?;

    for path in &artifact_files {
        set_timestamps(path).await.map_err(|err| {
            Status::internal(format!("failed to set artifact file timestamps: {err:?}"))
        })?;
    }

    Ok(())
}

fn expand_env(text: &str, envs: &[&String]) -> String {
    envs.iter().fold(text.to_string(), |acc, e| {
        let (key, value) = e.split_once('=').unwrap_or((e, ""));

        let mut output = String::with_capacity(acc.len());
        let mut copied = 0;

        for span in secrets::references(&acc, key) {
            output.push_str(&acc[copied..span.start]);
            output.push_str(value);
            copied = span.end;
        }

        output.push_str(&acc[copied..]);
        output
    })
}

/// Builds the sorted list of `KEY=value` environment variable strings for one step:
/// per-artifact `VORPAL_ARTIFACT_*` paths, `VORPAL_ARTIFACTS`, the step's own
/// `VORPAL_ARTIFACT_*`/`VORPAL_OUTPUT`/`VORPAL_WORKSPACE`, `VORPAL_SECRET_DIR` when the
/// step has secrets, and its custom environment variables. Secret values are never
/// part of it.
fn build_step_environments(
    artifact_digest: &str,
    artifact_namespace: &str,
    artifact_path: &Path,
    secret_dir_path: Option<&Path>,
    step_artifacts: &[String],
    step_environments: Vec<String>,
    workspace_path: &Path,
) -> Result<Vec<String>, Status> {
    let mut environments = vec![];

    // Add all artifact environment variables

    let mut paths = vec![];

    for artifact in step_artifacts {
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
        environments.push(format!("VORPAL_ARTIFACTS={}", paths.join(" ")));
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

    if let Some(secret_dir_path) = secret_dir_path {
        environments.push(format!("{SECRET_DIR_ENV}={}", secret_dir_path.display()));
    }

    // Add all custom environment variables

    environments.extend(step_environments);

    // Sort environment variables by key length

    environments.sort_by_key(std::string::String::len);

    Ok(environments)
}

/// Rejects a step whose secret names break the shared name rule, that sets
/// `VORPAL_SECRET_DIR` itself, or that references a secret in its arguments or
/// environment, where the expanded value would be visible to every host user in the
/// process arguments.
fn check_step_secrets(step: &ArtifactStep) -> Result<(), Status> {
    secrets::validate_names(step.secrets.iter().map(|secret| secret.name.as_str()))
        .map_err(|err| Status::invalid_argument(err.to_string()))?;

    if step
        .environments
        .iter()
        .any(|env| env.split_once('=').map_or(env.as_str(), |(key, _)| key) == SECRET_DIR_ENV)
    {
        return Err(Status::invalid_argument(format!(
            "step environment must not set {SECRET_DIR_ENV}"
        )));
    }

    for secret in &step.secrets {
        let referenced = step
            .arguments
            .iter()
            .chain(&step.environments)
            .any(|text| !secrets::references(text, &secret.name).is_empty());

        if referenced {
            return Err(Status::invalid_argument(format!(
                "step references secret {:?} in its arguments or environment; read it from ${SECRET_DIR_ENV}/{} instead",
                secret.name, secret.name
            )));
        }
    }

    Ok(())
}

/// Resolves `.`, `..` and repeated separators in a bwrap destination without touching
/// the filesystem. bwrap resolves a relative destination against the sandbox root.
fn normalize_sandbox_path(path: &str) -> PathBuf {
    let mut normalized = PathBuf::from("/");

    for component in Path::new(path).components() {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }

    normalized
}

/// Rejects bwrap `arguments` that mount or create something at `secret_dir_path` or one
/// of its ancestors, which would hide the secret directory from the step.
fn reject_mounts_over_secret_dir(
    arguments: &[String],
    secret_dir_path: &Path,
) -> Result<(), Status> {
    for (index, argument) in arguments.iter().enumerate() {
        let Some((_, count)) = BWRAP_DESTINATION_OPTIONS
            .iter()
            .find(|(option, _)| option == argument)
        else {
            continue;
        };

        let Some(destination) = arguments.get(index + count) else {
            continue;
        };

        if secret_dir_path.starts_with(normalize_sandbox_path(destination)) {
            return Err(Status::invalid_argument(format!(
                "step argument {argument} {destination:?} mounts over ${SECRET_DIR_ENV}"
            )));
        }
    }

    Ok(())
}

/// Opens each sealed secret with the private key at `private_key_path`, bound to
/// `artifact_namespace` and the secret's name, and writes it to its own file in a new
/// secret directory at `secret_dir_path`.
async fn write_step_secrets(
    artifact_namespace: &str,
    private_key_path: &Path,
    secret_dir_path: PathBuf,
    step_secrets: Vec<ArtifactStepSecret>,
) -> Result<SecretDir, Status> {
    let private_key = notary::get_private_key(private_key_path)
        .await
        .map_err(|err| Status::internal(format!("failed to load private key: {err:#}")))?;

    let secret_dir = SecretDir::create(secret_dir_path)
        .map_err(|err| Status::internal(format!("failed to create secret directory: {err}")))?;

    // Every error below names the secret only: its value must not reach the status.
    for secret in step_secrets {
        let associated_data = notary::secret_associated_data(artifact_namespace, &secret.name);

        let value = notary::open(&private_key, &secret.value, &associated_data).map_err(|err| {
            Status::internal(format!("failed to open secret '{}': {err}", secret.name))
        })?;

        secret_dir.write(&secret.name, &value).map_err(|err| {
            Status::internal(format!("failed to write secret '{}': {err}", secret.name))
        })?;
    }

    Ok(secret_dir)
}

/// Builds the command for one step, with its secrets written to a fresh secret
/// directory under `secret_parent_path`. The returned directory must outlive the
/// command's process. A rejected step creates no secret directory.
async fn prepare_step(
    artifact_digest: &str,
    artifact_namespace: &str,
    artifact_path: &Path,
    private_key_path: &Path,
    secret_parent_path: &Path,
    step: ArtifactStep,
    workspace_path: &Path,
) -> Result<(Command, Option<SecretDir>), Status> {
    check_step_secrets(&step)?;

    if !private_key_path.exists() {
        return Err(Status::internal("private key not found"));
    }

    let secret_dir_path =
        (!step.secrets.is_empty()).then(|| secret_parent_path.join(Uuid::now_v7().to_string()));

    let environments_sorted = build_step_environments(
        artifact_digest,
        artifact_namespace,
        artifact_path,
        secret_dir_path.as_deref(),
        &step.artifacts,
        step.environments,
        workspace_path,
    )?;

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

    for env in &environments_sorted {
        let (env_key, env_value) = env.split_once('=').unwrap_or((env, ""));
        let env_value = expand_env(env_value, &vorpal_envs);

        command.env(env_key, env_value);
    }

    // Setup arguments

    if !entrypoint.is_empty() {
        // Expand with all environment variables (VORPAL_ vars and custom envs)
        // Supports both ${VAR} and $VAR syntax
        let all_envs: Vec<_> = environments_sorted.iter().collect();

        let mut arguments: Vec<String> = step
            .arguments
            .iter()
            .map(|arg| expand_env(arg, &all_envs))
            .collect();

        // bwrap applies its options in order and stops at the script path, so the
        // secret bind goes after every other option, where no later mount can hide it.
        if let (Some(secret_dir_path), "bwrap") = (&secret_dir_path, entrypoint.as_str()) {
            if script_path.is_none() {
                return Err(Status::invalid_argument(
                    "a bwrap step with secrets needs a script to bind the secret directory before",
                ));
            }

            reject_mounts_over_secret_dir(&arguments, secret_dir_path)?;

            let secret_dir = secret_dir_path.display().to_string();

            arguments.extend([
                "--ro-bind".to_string(),
                secret_dir.clone(),
                secret_dir.clone(),
                "--setenv".to_string(),
                SECRET_DIR_ENV.to_string(),
                secret_dir,
            ]);
        }

        command.args(arguments);

        if let Some(script_path) = script_path {
            command.arg(script_path);
        }
    }

    let secret_dir = match secret_dir_path {
        Some(path) => Some(
            write_step_secrets(artifact_namespace, private_key_path, path, step.secrets).await?,
        ),
        None => None,
    };

    Ok((command, secret_dir))
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is an independent input to the step; grouping them would need a bespoke struct used by this one caller"
)]
async fn run_step(
    artifact_digest: &str,
    artifact_namespace: &str,
    artifact_path: &Path,
    private_key_path: &Path,
    secret_parent_path: &Path,
    step: ArtifactStep,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
    workspace_path: &Path,
) -> Result<(), Status> {
    // Held until this function returns, on every path, so the secrets outlive the process.
    let (mut command, _secret_dir) = prepare_step(
        artifact_digest,
        artifact_namespace,
        artifact_path,
        private_key_path,
        secret_parent_path,
        step,
        workspace_path,
    )
    .await?;

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

    let mut last_line = String::new();

    while let Some(line) = stdio_merged.next().await {
        let output =
            line.map_err(|err| Status::internal(format!("failed to read sandbox output: {err}")))?;

        // output is moved into the response below; last_line must persist
        // across loop iterations for the failure message after the loop.
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
        return Err(Status::internal(last_line));
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

/// Streams `artifact_archive`'s bytes to the registry's archive service under
/// `artifact_digest`/`artifact_namespace`.
async fn push_artifact_archive(
    artifact_archive: &std::path::PathBuf,
    artifact_digest: &str,
    artifact_namespace: &str,
    archive_auth_header: Option<&MetadataValue<Ascii>>,
    registry: &str,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    // Create authenticated archive client for pushing
    let client_archive_channel = build_channel(registry)
        .await
        .map_err(|e| Status::internal(format!("failed to connect to registry: {e}")))?;

    // Create client with authorization interceptor for pushing if token is available
    let mut client_archive = ArchiveServiceClient::with_interceptor(
        client_archive_channel,
        apply_auth_to_request(archive_auth_header),
    );

    send_message(format!("push: {artifact_digest}"), tx).await?;

    let artifact_file = tokio::fs::File::open(artifact_archive)
        .await
        .map_err(|err| Status::internal(format!("failed to open artifact archive: {err}")))?;

    let digest_for_stream = artifact_digest.to_string();
    let namespace_for_stream = artifact_namespace.to_string();

    let request_stream = async_stream::stream! {
        let mut reader = BufReader::new(artifact_file);
        let mut buf = vec![0u8; DEFAULT_CHUNKS_SIZE];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    // Each loop iteration yields an owned request; both fields
                    // are needed again on the next iteration.
                    yield ArchivePushRequest {
                        data: buf[..n].to_vec(),
                        digest: digest_for_stream.clone(),
                        namespace: namespace_for_stream.clone(),
                    };
                }
                Err(err) => {
                    error!("worker |> failed to read artifact archive chunk: {err}");
                    break;
                }
            }
        }
    };

    if let Err(err) = client_archive.push(request_stream).await {
        error!("worker |> failed to push artifact: {:?}", err);
        return Err(Status::internal(format!(
            "failed to push artifact: {err:?}"
        )));
    }

    Ok(())
}

/// Packs the built artifact's output files into a zstd archive, pushes the archive to
/// the registry, and stores the artifact record. Called only when the artifact
/// produced more than one output file (a single-file artifact is removed instead by
/// the caller).
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is independent context (paths, digests, auth headers, request payload) threaded through from build_artifact; grouping them would need a bespoke struct with no reuse beyond this call site"
)]
async fn pack_push_and_store_artifact(
    artifact: Artifact,
    artifact_digest: &str,
    artifact_output_path: &std::path::PathBuf,
    artifact_path_files: &[std::path::PathBuf],
    archive_auth_header: Option<&MetadataValue<Ascii>>,
    artifact_auth_header: Option<&MetadataValue<Ascii>>,
    registry: &str,
    request_artifact_aliases: Vec<String>,
    request_artifact_namespace: String,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    send_message(format!("pack: {artifact_digest}"), tx).await?;

    // Sanitize files

    for path in artifact_path_files {
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

    if let Err(err) =
        compress_zstd(artifact_output_path, artifact_path_files, &artifact_archive).await
    {
        error!("worker |> failed to compress artifact: {:?}", err);
        return Err(Status::internal(format!(
            "failed to compress artifact: {err:?}"
        )));
    }

    // TODO: check if archive is already uploaded

    // Upload archive

    push_artifact_archive(
        &artifact_archive,
        artifact_digest,
        &request_artifact_namespace,
        archive_auth_header,
        registry,
        tx,
    )
    .await?;

    // Store artifact in registry

    // Create authenticated artifact client
    let client_artifact_channel = build_channel(registry)
        .await
        .map_err(|e| Status::internal(format!("failed to connect to registry: {e}")))?;

    // Create client with authorization interceptor if token is available
    let mut client_artifact = ArtifactServiceClient::with_interceptor(
        client_artifact_channel,
        apply_auth_to_request(artifact_auth_header),
    );

    let request = StoreArtifactRequest {
        artifact: Some(artifact),
        artifact_aliases: request_artifact_aliases,
        artifact_namespace: request_artifact_namespace,
    };

    client_artifact
        .store_artifact(request)
        .await
        .map_err(|err| Status::internal(format!("failed to store artifact in registry: {err}")))?;

    // Remove artifact archive

    if let Err(err) = remove_file(&artifact_archive).await {
        error!("worker |> failed to remove artifact archive: {:?}", err);
        return Err(Status::internal(format!(
            "failed to remove artifact archive: {err:?}"
        )));
    }

    Ok(())
}

/// Validates `artifact` against `worker_target`, computes its digest, checks it is
/// neither already built nor locked by a concurrent build, and creates the lock file.
/// Returns the artifact's digest, output path, and lock path for the caller to use and
/// eventually remove.
async fn validate_and_lock_artifact(
    artifact: &Artifact,
    artifact_namespace: &str,
    artifact_json: &str,
) -> Result<(String, std::path::PathBuf, std::path::PathBuf), Status> {
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

    // Calculate artifact digest

    let artifact_digest = digest(artifact_json.as_bytes());

    // Check if artifact exists

    let artifact_output_path = get_artifact_output_path(&artifact_digest, artifact_namespace);

    if artifact_output_path.exists() {
        error!("worker |> artifact already exists: {}", artifact_digest);
        return Err(Status::already_exists("artifact exists"));
    }

    // Check if artifact is locked

    let artifact_output_lock = get_artifact_output_lock_path(&artifact_digest, artifact_namespace);

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

    Ok((artifact_digest, artifact_output_path, artifact_output_lock))
}

/// Pulls every declared source into `artifact_source_dir_path`, then pulls every
/// distinct dependency artifact (referenced by any step) into the local store.
async fn pull_sources_and_dependencies(
    artifact: &Artifact,
    artifact_namespace: &str,
    artifact_source_dir_path: &Path,
    archive_auth_header: Option<&MetadataValue<Ascii>>,
    registry: &str,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    for artifact_source in &artifact.sources {
        pull_source(
            archive_auth_header.cloned(),
            artifact_namespace.to_string(),
            artifact_source,
            artifact_source_dir_path,
            registry.to_string(),
            tx,
        )
        .await?;

        let source_digest = artifact_source
            .digest
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("source 'digest' is missing"))?;

        info!("worker |> pull source: {}", source_digest);
    }

    // Pull dependency artifacts

    let mut dependency_digests = HashSet::new();
    for step in &artifact.steps {
        for dep_digest in &step.artifacts {
            dependency_digests.insert(dep_digest.as_str());
        }
    }

    for dep_digest in &dependency_digests {
        pull_artifact(
            archive_auth_header,
            artifact_namespace,
            dep_digest,
            registry,
            tx,
        )
        .await?;

        info!("worker |> pull artifact: {}", dep_digest);
    }

    Ok(())
}

/// Obtains the pair of service-to-service `OAuth2` tokens `build_artifact` needs: one
/// scoped to the archive service, one to the artifact service.
async fn obtain_build_credentials(
    issuer: Option<&str>,
    issuer_audience: Option<&str>,
    issuer_client_id: Option<&str>,
    issuer_client_secret: Option<&str>,
) -> (Option<MetadataValue<Ascii>>, Option<MetadataValue<Ascii>>) {
    let archive_auth_header = obtain_service_credentials(
        issuer,
        issuer_audience,
        issuer_client_id,
        issuer_client_secret,
        "read:archive write:archive",
    )
    .await
    .map(|(token, _expires_in)| token);

    let artifact_auth_header = obtain_service_credentials(
        issuer,
        issuer_audience,
        issuer_client_id,
        issuer_client_secret,
        "read:artifact write:artifact",
    )
    .await
    .map(|(token, _expires_in)| token);

    (archive_auth_header, artifact_auth_header)
}

async fn build_artifact(
    issuer: Option<&str>,
    issuer_audience: Option<&str>,
    issuer_client_id: Option<&str>,
    issuer_client_secret: Option<&str>,
    request: BuildArtifactRequest,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    let artifact = request
        .artifact
        .ok_or_else(|| Status::invalid_argument("artifact is missing"))?;

    let artifact_namespace = &request.artifact_namespace;

    let artifact_json = serde_json::to_string(&artifact)
        .map_err(|err| Status::internal(format!("artifact failed to serialize: {err}")))?;

    let (artifact_digest, artifact_output_path, artifact_output_lock) =
        validate_and_lock_artifact(&artifact, artifact_namespace, &artifact_json).await?;
    let artifact_digest = &artifact_digest;

    // Obtain service-to-service OAuth2 tokens for archive and artifact services
    let (archive_auth_header, artifact_auth_header) = obtain_build_credentials(
        issuer,
        issuer_audience,
        issuer_client_id,
        issuer_client_secret,
    )
    .await;

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

    let registry = request.registry;

    pull_sources_and_dependencies(
        &artifact,
        artifact_namespace,
        &artifact_source_dir_path,
        archive_auth_header.as_ref(),
        &registry,
        tx,
    )
    .await?;

    // Run steps

    if let Err(err) = create_dir_all(&artifact_output_path).await {
        error!("worker |> failed to create artifact path: {:?}", err);
        return Err(Status::internal(format!(
            "failed to create artifact path: {err:?}"
        )));
    }

    let private_key_path = get_key_service_key_path();
    let secret_parent_path = get_root_secret_dir_path();

    // artifact is iterated by reference here and moved whole into
    // pack_push_and_store_artifact below; run_step needs an owned ArtifactStep.
    for step in &artifact.steps {
        if let Err(err) = run_step(
            artifact_digest,
            artifact_namespace,
            &artifact_output_path,
            &private_key_path,
            &secret_parent_path,
            step.clone(),
            tx,
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
        let request_artifact_aliases = request.artifact_aliases;
        let request_artifact_namespace = request.artifact_namespace;

        pack_push_and_store_artifact(
            artifact,
            artifact_digest,
            &artifact_output_path,
            &artifact_path_files,
            archive_auth_header.as_ref(),
            artifact_auth_header.as_ref(),
            &registry,
            request_artifact_aliases,
            request_artifact_namespace,
            tx,
        )
        .await?;
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
        // Check namespace authorization if auth is enabled. Service-user
        // tokens whose `azp` is in the trusted allow-list bypass namespace RBAC
        // per TDD §4.3 (m2m-authz-decoupling); human tokens still route through
        // `require_namespace_permission` unchanged.
        if request.extensions().get::<auth::Claims>().is_some() {
            let req_inner = request.get_ref();
            auth::require_namespace_or_service_trust(
                &request,
                &req_inner.artifact_namespace,
                "write",
            )?;

            // TDD §4.5 + AC §1.3 #5: every authenticated call records the
            // principal classification (Human with `sub`, TrustedService with
            // `azp`) and the namespace it touched.
            if let Some(auth::PrincipalKind::TrustedService { azp }) =
                request.extensions().get::<auth::PrincipalKind>()
            {
                info!(
                    "worker |> build_artifact by service={} in namespace {}",
                    azp, req_inner.artifact_namespace
                );
            } else {
                let user =
                    auth::get_user_context(&request).unwrap_or_else(|| "<unknown>".to_string());
                info!(
                    "worker |> build_artifact by user={} in namespace {}",
                    user, req_inner.artifact_namespace
                );
            }
        }

        let (tx, rx) = mpsc::channel(100);

        // self does not outlive the spawned future, which must own these fields.
        let issuer_audience = self.issuer_audience.clone();
        let issuer_client_id = self.issuer_client_id.clone();
        let issuer_client_secret = self.issuer_client_secret.clone();
        let issuer = self.issuer.clone();

        tokio::spawn(async move {
            if let Err(err) = build_artifact(
                issuer.as_deref(),
                issuer_audience.as_deref(),
                issuer_client_id.as_deref(),
                issuer_client_secret.as_deref(),
                request.into_inner(),
                &tx,
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

#[cfg(test)]
mod tests {
    use super::{expand_env, prepare_step, run_step, SecretDir, SECRET_DIR_ENV};
    use crate::command::store::notary::{
        self,
        test_fixtures::{certificate_chain_and_service_account, write_service_keys},
    };
    use std::{
        ffi::OsStr,
        io::ErrorKind,
        path::{Path, PathBuf},
    };
    use tempfile::TempDir;
    use tokio::{process::Command, sync::mpsc::channel};
    use tonic::Status;
    use vorpal_sdk::api::artifact::{ArtifactStep, ArtifactStepSecret};

    const MARKER: &str = "MARKER-5ec7e7-";

    /// Seals `value` with the public key at `public_key_path` for `namespace` and
    /// `name`, the way the agent prepares it.
    async fn sealed_secret(
        public_key_path: &Path,
        namespace: &str,
        name: &str,
        value: &[u8],
    ) -> anyhow::Result<Vec<u8>> {
        let public_key = notary::get_public_key(public_key_path).await?;
        let associated_data = notary::secret_associated_data(namespace, name);
        notary::seal(&public_key, value, &associated_data)
    }

    /// A service key pair generated at test run time.
    struct Keys {
        _dir: TempDir,
        public: PathBuf,
        private: PathBuf,
    }

    impl Keys {
        fn new() -> anyhow::Result<Self> {
            let dir = tempfile::tempdir()?;
            let (public, private) = write_service_keys(dir.path())?;
            Ok(Self {
                _dir: dir,
                public,
                private,
            })
        }

        /// The secret `name` sealed for namespace `team`.
        async fn secret(&self, name: &str, value: &[u8]) -> anyhow::Result<ArtifactStepSecret> {
            Ok(ArtifactStepSecret {
                name: name.to_string(),
                value: sealed_secret(&self.public, "team", name, value).await?,
            })
        }
    }

    /// One step's workspace, output and dedicated secret parent, under a temporary root.
    /// The secret parent is not created, so a test can see whether a step created it.
    struct StepDirs {
        root: TempDir,
        workspace: PathBuf,
        output: PathBuf,
        secret_parent: PathBuf,
    }

    impl StepDirs {
        fn new() -> anyhow::Result<Self> {
            let root = tempfile::tempdir()?;
            let workspace = root.path().join("workspace");
            let output = root.path().join("output");
            std::fs::create_dir(&workspace)?;
            std::fs::create_dir(&output)?;
            let secret_parent = root.path().join("secret");
            Ok(Self {
                root,
                workspace,
                output,
                secret_parent,
            })
        }

        /// The number of secret directories left under the secret parent.
        fn secret_dir_count(&self) -> anyhow::Result<usize> {
            match std::fs::read_dir(&self.secret_parent) {
                Ok(entries) => Ok(entries.count()),
                Err(err) if err.kind() == ErrorKind::NotFound => Ok(0),
                Err(err) => Err(err.into()),
            }
        }

        async fn prepare(
            &self,
            private_key_path: &Path,
            step: ArtifactStep,
        ) -> Result<(Command, Option<SecretDir>), Status> {
            prepare_step(
                "secret-test",
                "team",
                &self.output,
                private_key_path,
                &self.secret_parent,
                step,
                &self.workspace,
            )
            .await
        }

        async fn run(
            &self,
            namespace: &str,
            private_key_path: &Path,
            step: ArtifactStep,
        ) -> Result<(), Status> {
            let (tx, _rx) = channel(16);
            run_step(
                "secret-test",
                namespace,
                &self.output,
                private_key_path,
                &self.secret_parent,
                step,
                &tx,
                &self.workspace,
            )
            .await
        }
    }

    fn command_args(command: &Command) -> Vec<String> {
        command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn command_env(command: &Command, key: &str) -> Option<String> {
        command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == OsStr::new(key))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    }

    /// Runs one step in `namespace` whose script copies `$VORPAL_SECRET_DIR/SECRET` to
    /// a file, with `value` sealed as the step's `SECRET`. Returns the bytes the step
    /// wrote, or the step's error after checking the step never ran. Either way the
    /// secret directory is gone afterwards.
    async fn run_secret_step(
        private_key_path: &Path,
        namespace: &str,
        value: Vec<u8>,
    ) -> anyhow::Result<Result<Vec<u8>, Status>> {
        let dirs = StepDirs::new()?;
        let step = ArtifactStep {
            script: Some(
                "#!/bin/sh\ncat \"$VORPAL_SECRET_DIR/SECRET\" > \"$VORPAL_OUTPUT/secret\"\n".into(),
            ),
            secrets: vec![ArtifactStepSecret {
                name: "SECRET".to_string(),
                value,
            }],
            ..Default::default()
        };

        let result = dirs.run(namespace, private_key_path, step).await;

        assert_eq!(dirs.secret_dir_count()?, 0, "a secret directory remains");
        let written = dirs.output.join("secret");
        match result {
            Ok(()) => Ok(Ok(std::fs::read(written)?)),
            Err(status) => {
                assert!(!written.exists(), "the step ran despite: {status:?}");
                Ok(Err(status))
            }
        }
    }

    #[tokio::test]
    async fn step_reads_sealed_secrets_unchanged_from_the_secret_directory() -> anyhow::Result<()> {
        let keys = tempfile::tempdir()?;
        let (public_key_path, private_key_path) = write_service_keys(keys.path())?;
        let [certificate_chain, service_account] = certificate_chain_and_service_account()?;
        let one_kib = "key=value/".repeat(103);
        assert!(one_kib.len() >= 1024);

        for value in [one_kib, certificate_chain, service_account] {
            let sealed =
                sealed_secret(&public_key_path, "team", "SECRET", value.as_bytes()).await?;

            let delivered = run_secret_step(&private_key_path, "team", sealed).await??;

            assert_eq!(delivered, value.as_bytes());
        }
        Ok(())
    }

    #[tokio::test]
    async fn step_reads_binary_secret_byte_for_byte_from_its_file() -> anyhow::Result<()> {
        let keys = tempfile::tempdir()?;
        let (public_key_path, private_key_path) = write_service_keys(keys.path())?;
        let value = [MARKER.as_bytes(), &[0x00, 0xff, 0xfe, 0x80]].concat();
        assert!(std::str::from_utf8(&value).is_err());
        let sealed = sealed_secret(&public_key_path, "team", "SECRET", &value).await?;

        let delivered = run_secret_step(&private_key_path, "team", sealed).await??;

        assert_eq!(delivered, value);
        Ok(())
    }

    #[tokio::test]
    async fn bwrap_step_binds_the_secret_directory_after_its_options_before_the_script(
    ) -> anyhow::Result<()> {
        let keys = Keys::new()?;
        let dirs = StepDirs::new()?;
        let step = ArtifactStep {
            entrypoint: Some("bwrap".to_string()),
            arguments: vec!["--clearenv".into(), "--tmpfs".into(), "/scratch".into()],
            script: Some("#!/bin/sh\n".into()),
            secrets: vec![keys.secret("TOKEN", b"value").await?],
            ..Default::default()
        };

        let (command, secret_dir) = dirs.prepare(&keys.private, step).await?;

        assert!(secret_dir.is_some());
        let secret_dir = command_env(&command, SECRET_DIR_ENV).unwrap_or_default();
        let script = dirs.workspace.join("script.sh").display().to_string();
        assert_eq!(
            command_args(&command),
            [
                "--clearenv",
                "--tmpfs",
                "/scratch",
                "--ro-bind",
                &secret_dir,
                &secret_dir,
                "--setenv",
                SECRET_DIR_ENV,
                &secret_dir,
                &script,
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn step_without_secrets_creates_no_secret_directory() -> anyhow::Result<()> {
        let keys = Keys::new()?;
        let dirs = StepDirs::new()?;
        let step = ArtifactStep {
            entrypoint: Some("bwrap".to_string()),
            script: Some("#!/bin/sh\n".into()),
            ..Default::default()
        };

        let (command, secret_dir) = dirs.prepare(&keys.private, step).await?;

        assert!(secret_dir.is_none());
        assert!(!dirs.secret_parent.exists());
        assert_eq!(command_env(&command, SECRET_DIR_ENV), None);
        assert!(!command_args(&command).contains(&SECRET_DIR_ENV.to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn environment_values_keep_equals_signs_and_entries_without_one_are_empty(
    ) -> anyhow::Result<()> {
        let keys = Keys::new()?;
        let dirs = StepDirs::new()?;
        let step = ArtifactStep {
            entrypoint: Some("/bin/sh".to_string()),
            arguments: vec!["$FOO|$FLAG".into()],
            environments: vec!["FOO=a=b==".into(), "FLAG".into()],
            ..Default::default()
        };

        let (command, _) = dirs.prepare(&keys.private, step).await?;

        assert_eq!(command_env(&command, "FOO").as_deref(), Some("a=b=="));
        assert_eq!(command_env(&command, "FLAG").as_deref(), Some(""));
        assert_eq!(command_args(&command), ["a=b==|"]);
        Ok(())
    }

    mod abuse {
        use super::{
            command_args, command_env, run_secret_step, sealed_secret, Keys, StepDirs, MARKER,
        };
        use crate::command::{
            start::worker::{remove_orphaned_secret_dirs, SecretDir, SECRET_DIR_ENV},
            store::notary::{self, test_fixtures::write_service_keys},
        };
        use rsa::{rand_core::OsRng, Pkcs1v15Encrypt};
        use std::os::unix::fs::PermissionsExt;
        use tonic::Code;
        use vorpal_sdk::api::artifact::{ArtifactStep, ArtifactStepSecret};

        fn bwrap_step(arguments: &[&str], secrets: Vec<ArtifactStepSecret>) -> ArtifactStep {
            ArtifactStep {
                entrypoint: Some("bwrap".to_string()),
                arguments: arguments.iter().map(ToString::to_string).collect(),
                script: Some("#!/bin/sh\n".into()),
                secrets,
                ..Default::default()
            }
        }

        #[tokio::test]
        async fn step_passing_a_secret_through_arguments_or_environment_fails_before_spawn(
        ) -> anyhow::Result<()> {
            let keys = Keys::new()?;

            for (arguments, environments) in [
                (vec!["--setenv", "NAME", "$NAME"], vec![]),
                (vec!["--setenv", "FOO", "x${NAME}x"], vec![]),
                (vec![], vec!["FOO=$NAME"]),
            ] {
                let dirs = StepDirs::new()?;
                let mut step = bwrap_step(
                    &arguments,
                    vec![keys.secret("NAME", MARKER.as_bytes()).await?],
                );
                step.environments = environments.iter().map(ToString::to_string).collect();

                let Err(status) = dirs.prepare(&keys.private, step).await else {
                    anyhow::bail!("{arguments:?} {environments:?} passed a secret to bwrap");
                };

                assert_eq!(status.code(), Code::InvalidArgument);
                assert!(status.message().contains("\"NAME\""), "{status:?}");
                assert!(!format!("{status:?}").contains(MARKER), "{status:?}");
                assert_eq!(dirs.secret_dir_count()?, 0);
            }
            Ok(())
        }

        #[tokio::test]
        async fn unreferenced_secret_is_in_no_argument_or_environment_entry() -> anyhow::Result<()>
        {
            let keys = Keys::new()?;

            for entrypoint in ["bwrap", "/bin/sh"] {
                let dirs = StepDirs::new()?;
                let step = ArtifactStep {
                    entrypoint: Some(entrypoint.to_string()),
                    arguments: vec!["--setenv".into(), "HOME".into(), "$VORPAL_WORKSPACE".into()],
                    environments: vec!["PATH=/usr/bin:/bin".into()],
                    script: Some("#!/bin/sh\n".into()),
                    secrets: vec![keys.secret("NAME", MARKER.as_bytes()).await?],
                    ..Default::default()
                };

                let (command, secret_dir) = dirs.prepare(&keys.private, step).await?;

                let Some(secret_dir) = secret_dir else {
                    anyhow::bail!("{entrypoint}: no secret directory for a step with a secret");
                };
                let secret_dir = &secret_dir.path;
                assert_eq!(std::fs::read(secret_dir.join("NAME"))?, MARKER.as_bytes());
                for arg in command_args(&command) {
                    assert!(!arg.contains(MARKER), "{entrypoint}: argument {arg:?}");
                }
                for (key, value) in command.as_std().get_envs() {
                    let value = value.map_or_default(|v| v.to_string_lossy());
                    assert!(!value.contains(MARKER), "{entrypoint}: {key:?}={value:?}");
                }
                assert_eq!(
                    command_env(&command, SECRET_DIR_ENV),
                    Some(secret_dir.display().to_string())
                );
            }
            Ok(())
        }

        #[tokio::test]
        async fn secret_directory_and_files_are_private_and_outside_workspace_and_output(
        ) -> anyhow::Result<()> {
            let keys = Keys::new()?;
            let dirs = StepDirs::new()?;
            let step = bwrap_step(&[], vec![keys.secret("NAME", MARKER.as_bytes()).await?]);

            let (_, secret_dir) = dirs.prepare(&keys.private, step).await?;

            let Some(secret_dir) = secret_dir else {
                anyhow::bail!("no secret directory for a step with a secret");
            };
            let mode = |path: &std::path::Path| -> anyhow::Result<u32> {
                Ok(std::fs::symlink_metadata(path)?.permissions().mode() & 0o7777)
            };
            assert_eq!(mode(&dirs.secret_parent)?, 0o700);
            assert_eq!(mode(&secret_dir.path)?, 0o700);
            assert_eq!(mode(&secret_dir.path.join("NAME"))?, 0o600);
            assert!(secret_dir.path.starts_with(&dirs.secret_parent));
            assert!(!secret_dir.path.starts_with(&dirs.workspace));
            assert!(!secret_dir.path.starts_with(&dirs.output));
            Ok(())
        }

        #[tokio::test]
        async fn secret_directory_is_removed_after_a_step_that_exits_non_zero() -> anyhow::Result<()>
        {
            let keys = Keys::new()?;
            let dirs = StepDirs::new()?;
            let step = ArtifactStep {
                script: Some(
                    "#!/bin/sh\ntest -f \"$VORPAL_SECRET_DIR/NAME\" && exit 7\nexit 0\n".into(),
                ),
                secrets: vec![keys.secret("NAME", b"value").await?],
                ..Default::default()
            };

            let result = dirs.run("team", &keys.private, step).await;

            assert!(result.is_err(), "the step did not fail: {result:?}");
            assert!(dirs.secret_parent.exists());
            assert_eq!(dirs.secret_dir_count()?, 0);
            Ok(())
        }

        #[tokio::test]
        async fn secret_directory_is_removed_after_a_step_whose_spawn_fails() -> anyhow::Result<()>
        {
            let keys = Keys::new()?;
            let dirs = StepDirs::new()?;
            let step = ArtifactStep {
                entrypoint: Some(dirs.root.path().join("missing").display().to_string()),
                secrets: vec![keys.secret("NAME", b"value").await?],
                ..Default::default()
            };

            let Err(status) = dirs.run("team", &keys.private, step).await else {
                anyhow::bail!("a missing entrypoint spawned");
            };

            assert!(status.message().contains("spawn"), "{status:?}");
            assert!(dirs.secret_parent.exists());
            assert_eq!(dirs.secret_dir_count()?, 0);
            Ok(())
        }

        #[tokio::test]
        async fn bwrap_step_mounting_over_the_secret_directory_or_an_ancestor_is_rejected(
        ) -> anyhow::Result<()> {
            let keys = Keys::new()?;
            let dirs = StepDirs::new()?;
            let parent = dirs.secret_parent.display().to_string();

            for arguments in [
                vec!["--tmpfs", "$VORPAL_SECRET_DIR"],
                vec!["--tmpfs", "$VORPAL_SECRET_DIR/.."],
                vec!["--bind", "/tmp", "$VORPAL_SECRET_DIR/./"],
                vec!["--ro-bind", "/tmp", &format!("{parent}/")],
                vec!["--dir", &format!("{parent}//x/..")],
                vec!["--setenv", "A", "B", "--overlay", "/a", "/b", "/"],
                vec!["--bind-data", "3", "/"],
                vec!["--remount-ro", "/"],
            ] {
                let step = bwrap_step(&arguments, vec![keys.secret("NAME", b"value").await?]);

                let Err(status) = dirs.prepare(&keys.private, step).await else {
                    anyhow::bail!("{arguments:?} mounted over the secret directory");
                };

                assert_eq!(status.code(), Code::InvalidArgument, "{arguments:?}");
                assert!(status.message().contains(SECRET_DIR_ENV), "{status:?}");
                assert_eq!(dirs.secret_dir_count()?, 0);
            }

            for arguments in [
                vec!["--tmpfs", "/scratch"],
                vec!["--ro-bind", &parent, "/elsewhere"],
                vec!["--tmpfs", "$VORPAL_SECRET_DIR/sub"],
            ] {
                let step = bwrap_step(&arguments, vec![keys.secret("NAME", b"value").await?]);
                let prepared = dirs.prepare(&keys.private, step).await;
                assert!(prepared.is_ok(), "{arguments:?}: {:?}", prepared.err());
            }
            Ok(())
        }

        #[tokio::test]
        async fn invalid_secret_name_fails_before_decryption_and_creates_nothing(
        ) -> anyhow::Result<()> {
            let dirs = StepDirs::new()?;
            let missing_key = dirs.root.path().join("missing.pem");

            for name in ["../x", "a/b", "", "VORPAL_X", "a\nb"] {
                let step = bwrap_step(
                    &[],
                    vec![ArtifactStepSecret {
                        name: name.to_string(),
                        value: MARKER.as_bytes().to_vec(),
                    }],
                );

                let Err(status) = dirs.prepare(&missing_key, step).await else {
                    anyhow::bail!("secret name {name:?} was accepted");
                };

                assert_eq!(status.code(), Code::InvalidArgument, "{status:?}");
                assert!(
                    status.message().contains(&format!("{name:?}")),
                    "{status:?}"
                );
                assert!(!dirs.secret_parent.exists(), "{name:?}");
            }
            Ok(())
        }

        #[tokio::test]
        async fn repeated_secret_name_fails_before_spawn() -> anyhow::Result<()> {
            let keys = Keys::new()?;
            let dirs = StepDirs::new()?;
            let step = ArtifactStep {
                script: Some("#!/bin/sh\ntouch \"$VORPAL_OUTPUT/ran\"\n".into()),
                secrets: vec![
                    keys.secret("A", b"first").await?,
                    keys.secret("A", b"second").await?,
                ],
                ..Default::default()
            };

            let Err(status) = dirs.run("team", &keys.private, step).await else {
                anyhow::bail!("a repeated secret name was accepted");
            };

            assert!(status.message().contains("\"A\""), "{status:?}");
            assert!(!dirs.output.join("ran").exists());
            assert!(!dirs.secret_parent.exists());
            Ok(())
        }

        #[tokio::test]
        async fn environment_entry_setting_the_secret_directory_is_rejected() -> anyhow::Result<()>
        {
            let keys = Keys::new()?;

            for secrets in [vec![], vec![keys.secret("NAME", b"value").await?]] {
                let dirs = StepDirs::new()?;
                let mut step = bwrap_step(&[], secrets);
                step.environments = vec![format!("{SECRET_DIR_ENV}=/tmp/x")];

                let Err(status) = dirs.prepare(&keys.private, step).await else {
                    anyhow::bail!("the step set {SECRET_DIR_ENV} itself");
                };

                assert_eq!(status.code(), Code::InvalidArgument);
                assert!(status.message().contains(SECRET_DIR_ENV), "{status:?}");
                assert!(!dirs.secret_parent.exists());
            }
            Ok(())
        }

        #[tokio::test]
        async fn startup_cleanup_removes_unheld_secret_directories_and_keeps_held_ones(
        ) -> anyhow::Result<()> {
            let dirs = StepDirs::new()?;
            let held = SecretDir::create(dirs.secret_parent.join("held"))?;
            held.write("NAME", b"live")?;
            let orphan = dirs.secret_parent.join("orphan");
            std::fs::create_dir(&orphan)?;
            std::fs::write(orphan.join("NAME"), b"left behind")?;

            remove_orphaned_secret_dirs(&dirs.secret_parent)?;

            assert!(held.path.join("NAME").exists());
            assert!(!orphan.exists());
            drop(held);
            assert_eq!(dirs.secret_dir_count()?, 0);
            Ok(())
        }

        #[test]
        fn startup_cleanup_tolerates_a_missing_secret_parent() -> anyhow::Result<()> {
            let dirs = StepDirs::new()?;
            remove_orphaned_secret_dirs(&dirs.secret_parent)?;
            Ok(())
        }

        #[tokio::test]
        async fn step_fails_on_secret_in_older_format_naming_it_without_its_value(
        ) -> anyhow::Result<()> {
            let keys = tempfile::tempdir()?;
            let (public_key_path, private_key_path) = write_service_keys(keys.path())?;
            let public_key = notary::get_public_key(&public_key_path).await?;
            let older_format =
                public_key.encrypt(&mut OsRng, Pkcs1v15Encrypt, MARKER.as_bytes())?;

            let Err(status) = run_secret_step(&private_key_path, "team", older_format).await?
            else {
                anyhow::bail!("the step ran with a secret in the older format");
            };

            let message = status.message();
            assert!(message.contains("'SECRET'"), "{message}");
            assert!(message.contains("older format"), "{message}");
            assert!(!format!("{status:?}").contains(MARKER), "{status:?}");
            Ok(())
        }

        #[tokio::test]
        async fn step_fails_on_secret_sealed_for_a_different_name() -> anyhow::Result<()> {
            let keys = tempfile::tempdir()?;
            let (public_key_path, private_key_path) = write_service_keys(keys.path())?;
            let sealed = sealed_secret(&public_key_path, "team", "OTHER", b"bound secret").await?;

            let Err(status) = run_secret_step(&private_key_path, "team", sealed).await? else {
                anyhow::bail!("the step ran with a secret sealed for another name");
            };

            assert!(status.message().contains("'SECRET'"), "{status:?}");
            assert!(status.message().contains("failed to open"), "{status:?}");
            Ok(())
        }

        #[tokio::test]
        async fn step_fails_on_secret_sealed_for_a_different_namespace() -> anyhow::Result<()> {
            let keys = tempfile::tempdir()?;
            let (public_key_path, private_key_path) = write_service_keys(keys.path())?;
            let sealed = sealed_secret(&public_key_path, "team", "SECRET", b"bound secret").await?;

            let Err(status) = run_secret_step(&private_key_path, "other", sealed).await? else {
                anyhow::bail!("the step ran with a secret sealed for another namespace");
            };

            assert!(status.message().contains("'SECRET'"), "{status:?}");
            assert!(status.message().contains("failed to open"), "{status:?}");
            Ok(())
        }
    }

    fn expand_with_token(text: &str) -> String {
        let token = String::from("TOKEN=s3cr3t");
        expand_env(text, &[&token])
    }

    #[test]
    fn expands_braced_and_delimited_references() {
        assert_eq!(expand_with_token("$TOKEN"), "s3cr3t");
        assert_eq!(expand_with_token("${TOKEN}"), "s3cr3t");
        assert_eq!(expand_with_token("${TOKEN}x"), "s3cr3tx");
        assert_eq!(expand_with_token("x$TOKEN.y"), "xs3cr3t.y");
        assert_eq!(expand_with_token("$TOKEN-a"), "s3cr3t-a");
        assert_eq!(
            expand_with_token("a $TOKEN b ${TOKEN}"),
            "a s3cr3t b s3cr3t"
        );
    }

    #[test]
    fn leaves_longer_names_and_brace_suffixes_unexpanded() {
        assert_eq!(expand_with_token("$TOKENX"), "$TOKENX");
        assert_eq!(expand_with_token("$TOKEN_1"), "$TOKEN_1");
        assert_eq!(expand_with_token("$TOKEN{"), "$TOKEN{");
        assert_eq!(expand_with_token("$TOKENé"), "$TOKENé");
        assert_eq!(expand_with_token("${TOKENX}"), "${TOKENX}");
    }

    #[test]
    fn expands_to_the_whole_value_after_the_first_equals_sign() {
        let token = String::from("TOKEN=a=b==");
        assert_eq!(expand_env("x $TOKEN y", &[&token]), "x a=b== y");
    }
}
