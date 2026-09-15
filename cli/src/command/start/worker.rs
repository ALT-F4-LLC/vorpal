use crate::command::{
    start::auth,
    store::{
        archives::{compress_zstd, unpack_zstd},
        notary,
        paths::{
            get_artifact_archive_path, get_artifact_output_lock_path, get_artifact_output_path,
            get_file_paths, get_key_service_key_path, set_timestamps,
        },
        temps::{create_sandbox_dir, create_sandbox_file},
    },
};
use anyhow::Result;
use sha256::digest;
use std::{
    collections::HashSet, fs::Permissions, os::unix::fs::PermissionsExt, path::Path, process::Stdio,
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
        Self {
            issuer_audience,
            issuer_client_id,
            issuer_client_secret,
            issuer,
        }
    }
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
        let parts = e.split('=').collect::<Vec<&str>>();
        let key = parts[0];
        let value = parts[1];

        // First, replace ${VAR} syntax (braced)
        let result = acc.replace(&format!("${{{key}}}"), value);

        // Then, replace $VAR syntax (unbraced) while preserving ${{VAR}} and $VARNAME patterns
        let search = format!("${key}");
        let mut output = String::new();
        let mut i = 0;

        while i < result.len() {
            if result[i..].starts_with(&search) {
                let after_idx = i + search.len();

                // Check what comes after $KEY
                match result[after_idx..].chars().next() {
                    // End of string - replace it
                    None => {
                        output.push_str(value);
                        i = after_idx;
                    }
                    Some('{') => {
                        // This is $VAR{ or part of ${{VAR}} - don't replace
                        output.push_str(&search);
                        i += search.len();
                    }
                    Some(next_char) if next_char.is_alphanumeric() || next_char == '_' => {
                        // Part of longer variable name like $API_SECRETA - don't replace
                        output.push_str(&search);
                        i += search.len();
                    }
                    Some(_) => {
                        // Followed by delimiter (space, quote, slash, etc.) - replace it
                        output.push_str(value);
                        i = after_idx;
                    }
                }
            } else {
                // `i < result.len()` (the loop guard), so a char always starts at `i`.
                let Some(current_char) = result[i..].chars().next() else {
                    break;
                };
                output.push(current_char);
                i += current_char.len_utf8();
            }
        }

        output
    })
}

/// Builds the sorted list of `KEY=value` environment variable strings for one step:
/// per-artifact `VORPAL_ARTIFACT_*` paths, `VORPAL_ARTIFACTS`, the step's own
/// `VORPAL_ARTIFACT_*`/`VORPAL_OUTPUT`/`VORPAL_WORKSPACE`, its custom environment
/// variables, and its secrets (decrypted with the service private key).
async fn build_step_environments(
    artifact_digest: &str,
    artifact_namespace: &str,
    artifact_path: &Path,
    step_artifacts: &[String],
    step_environments: Vec<String>,
    step_secrets: Vec<ArtifactStepSecret>,
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

    // Add all custom environment variables

    environments.extend(step_environments);

    // Add all secrets as environment variables

    let private_key_path = get_key_service_key_path();

    if !private_key_path.exists() {
        return Err(Status::internal("private key not found"));
    }

    for secret in step_secrets {
        let value = notary::decrypt(&private_key_path, secret.value)
            .await
            .map_err(|err| Status::internal(format!("failed to decrypt secret: {err}")))?;

        environments.push(format!("{}={}", secret.name, value));
    }

    // Sort environment variables by key length

    environments.sort_by_key(std::string::String::len);

    Ok(environments)
}

async fn run_step(
    artifact_digest: &str,
    artifact_namespace: &str,
    artifact_path: &Path,
    step: ArtifactStep,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
    workspace_path: &Path,
) -> Result<(), Status> {
    let environments_sorted = build_step_environments(
        artifact_digest,
        artifact_namespace,
        artifact_path,
        &step.artifacts,
        step.environments,
        step.secrets,
        workspace_path,
    )
    .await?;

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
        let env = env.split('=').collect::<Vec<&str>>();
        let env_value = expand_env(env[1], &vorpal_envs);

        command.env(env[0], env_value);
    }

    // Setup arguments

    if !entrypoint.is_empty() {
        // Create references to all environments for expansion (includes VORPAL_ vars, custom envs, and secrets)
        let all_envs: Vec<_> = environments_sorted.iter().collect();

        for arg in &step.arguments {
            // Expand with all environment variables (VORPAL_ vars, custom envs, and secrets)
            // Supports both ${VAR} and $VAR syntax
            let arg = expand_env(arg, &all_envs);

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

    // artifact is iterated by reference here and moved whole into
    // pack_push_and_store_artifact below; run_step needs an owned ArtifactStep.
    for step in &artifact.steps {
        if let Err(err) = run_step(
            artifact_digest,
            artifact_namespace,
            &artifact_output_path,
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
