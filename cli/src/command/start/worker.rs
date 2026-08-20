use crate::command::{
    start::auth,
    store::{
        archives::{compress_zstd, unpack_zstd},
        notary,
        paths::{
            discard_staging, get_artifact_archive_path, get_artifact_output_lock_path,
            get_artifact_output_path, get_file_paths, get_key_service_key_path, publish_atomically,
            set_timestamps, staging_path_for, PublishOutcome,
        },
        temps::{create_sandbox_dir, create_sandbox_file},
    },
};
use anyhow::Result;
use sha256::digest;
use std::{
    collections::HashSet,
    fs::Permissions,
    future::Future,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    fs::{
        create_dir_all, read_link, remove_dir_all, remove_file, set_permissions, symlink_metadata,
        write, File,
    },
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
use walkdir::{DirEntry, WalkDir};

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

                publish_archive(&response_data, &source_archive).await?;
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

/// Writes `data` to the shared `archive_path` (recipe-addressed, not
/// content-addressed — see `publish_atomically`), staging it first so no
/// reader ever opens a half-written archive.
async fn publish_archive(data: &[u8], archive_path: &Path) -> Result<(), Status> {
    let archive_parent = archive_path.parent().ok_or_else(|| {
        Status::internal(format!(
            "failed to get parent of archive {}",
            archive_path.display()
        ))
    })?;

    create_dir_all(archive_parent).await.map_err(|err| {
        Status::internal(format!(
            "failed to create archive parent {}: {err}",
            archive_parent.display()
        ))
    })?;

    let staging_path = staging_path_for(archive_path);

    let staged = async {
        write(&staging_path, data).await.map_err(|err| {
            Status::internal(format!(
                "failed to write archive {}: {err}",
                archive_path.display()
            ))
        })?;

        set_timestamps(&staging_path).await.map_err(|err| {
            Status::internal(format!(
                "failed to set timestamps on archive {}: {err}",
                archive_path.display()
            ))
        })?;

        publish_atomically(&staging_path, archive_path)
            .await
            .map(|_| ())
            .map_err(|err| Status::internal(err.to_string()))
    }
    .await;

    if staged.is_err() {
        discard_staging(&staging_path).await;
    }

    staged
}

/// Unpacks `archive_path` into the shared `output_path` (recipe-addressed,
/// not content-addressed — see `publish_atomically`), staging it first so
/// the real path is only ever created — whole — by the final rename.
/// An archive that unpacks to no files is refused here for the same reason a
/// build that produced none is: `stage_then_publish` owns that rule, and a
/// pull is the other way an empty entry reaches a shared store path.
async fn publish_unpacked(archive_path: &Path, output_path: &Path) -> Result<(), Status> {
    stage_then_publish(output_path, move |staging_path| async move {
        unpack_zstd(&staging_path, archive_path)
            .await
            .map_err(|err| {
                Status::internal(format!("failed to unpack artifact archive: {err:?}"))
            })?;

        for entry in staged_entries(&staging_path)? {
            set_timestamps(&entry.path().to_path_buf())
                .await
                .map_err(|err| {
                    Status::internal(format!("failed to set artifact file timestamps: {err:?}"))
                })?;
        }

        Ok(())
    })
    .await
    .map(|_| ())
}

/// Every entry staged under `staging_path`, the root included, in walk order.
///
/// Walked here rather than through `get_file_paths`: that walker builds a
/// packing list, so it drops `.git` entries and turns a walk error into a
/// silently missing subtree. The publish renames the whole staging directory,
/// so an entry missing from this list is published unscanned and is absent
/// from the archive pushed for the same digest — two workers would then hold
/// different content under one digest.
fn staged_entries(staging_path: &Path) -> Result<Vec<DirEntry>, Status> {
    WalkDir::new(staging_path)
        .into_iter()
        .map(|entry| {
            entry.map_err(|err| Status::internal(format!("failed to read staged output: {err}")))
        })
        .collect()
}

/// The staged entries that carry an artifact's bytes: regular files and
/// symlinks. A tree of directories alone carries none, so there is nothing in
/// it for a dependent to use.
fn staged_content_paths(entries: &[DirEntry]) -> Vec<PathBuf> {
    entries
        .iter()
        .filter(|entry| entry.file_type().is_file() || entry.file_type().is_symlink())
        .map(|entry| entry.path().to_path_buf())
        .collect()
}

/// Produces a store entry under a private staging directory and publishes it
/// to `output_path` with a single rename.
///
/// `produce` is handed the staging directory and never learns `output_path`:
/// the real path comes into existence exactly once, in the publish below. A
/// producer that fails, or a worker that is killed, therefore strands only the
/// staging directory, and no reader's `exists()` check on the real path can
/// mistake a half-built entry for a finished one.
///
/// A producer that wrote no files does not publish — files and symlinks are
/// what carry an artifact's bytes, so a tree of directories alone counts as
/// nothing. Such a tree passes every one of those `exists()` readers as a
/// complete entry and would cache a build that produced no output forever, so
/// it is discarded and the failure reported. Callers must release whatever
/// they hold for the digest when that happens, or the refusal denies the
/// digest instead of leaving it buildable.
async fn stage_then_publish<F, Fut>(
    output_path: &Path,
    produce: F,
) -> Result<PublishOutcome, Status>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<(), Status>>,
{
    let staging_path = staging_path_for(output_path);

    create_dir_all(&staging_path).await.map_err(|err| {
        error!(
            "worker |> failed to create staging path {}: {err}",
            staging_path.display()
        );

        Status::internal("failed to create staging path")
    })?;

    let staged = async {
        produce(staging_path.clone()).await?;

        // The producer owned this path for its whole run and could have put a
        // symlink in its place. Reads follow symlinks and the publish rename
        // does not, so a check made through a link would describe one tree
        // while the link itself is installed as the store entry.
        let staged_metadata = symlink_metadata(&staging_path)
            .await
            .map_err(|err| Status::internal(format!("failed to stat staged output: {err}")))?;

        if !staged_metadata.is_dir() {
            return Err(Status::internal("staged output is no longer a directory"));
        }

        let entries = staged_entries(&staging_path)?;

        if staged_content_paths(&entries).is_empty() {
            return Err(Status::internal("artifact produced no output files"));
        }

        publish_atomically(&staging_path, output_path)
            .await
            .map_err(|err| Status::internal(err.to_string()))
    }
    .await;

    if staged.is_err() {
        discard_staging(&staging_path).await;
    }

    staged
}

/// Size of one scan read. Deliberately its own constant: this is a local file
/// read, and it agrees with the gRPC chunk limit only by coincidence.
const SCAN_CHUNK_SIZE: usize = 8192;

/// Reports the first entry under a staged tree that carries `needle` in its
/// bytes — file contents or symlink target.
///
/// Files are read in overlapping chunks so a match straddling a chunk boundary
/// is still found and an artifact larger than memory is still scannable.
///
/// This finds a literal occurrence, so it binds a cooperating producer only: a
/// step that encodes, compresses or splits the path defeats it, as does one
/// that writes the path after the scan has run. It is a regression guard
/// against an artifact accidentally recording where it was built, not a
/// control against a hostile step.
async fn find_embedded_reference(
    staged_files: &[PathBuf],
    needle: &str,
) -> Result<Option<PathBuf>, Status> {
    let needle = needle.as_bytes();
    let overlap = needle.len() - 1;

    for path in staged_files.iter() {
        let metadata = symlink_metadata(path)
            .await
            .map_err(|err| Status::internal(format!("failed to stat output file: {err}")))?;

        if metadata.is_symlink() {
            let target = read_link(path)
                .await
                .map_err(|err| Status::internal(format!("failed to read output link: {err}")))?;

            if target
                .as_os_str()
                .as_bytes()
                .windows(needle.len())
                .any(|w| w == needle)
            {
                return Ok(Some(path.clone()));
            }

            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let mut reader = BufReader::new(
            File::open(path)
                .await
                .map_err(|err| Status::internal(format!("failed to open output file: {err}")))?,
        );

        let mut buf = vec![0u8; SCAN_CHUNK_SIZE + overlap];
        let mut carried = 0usize;

        loop {
            let read = reader
                .read(&mut buf[carried..])
                .await
                .map_err(|err| Status::internal(format!("failed to read output file: {err}")))?;

            if read == 0 {
                break;
            }

            let filled = carried + read;

            if buf[..filled].windows(needle.len()).any(|w| w == needle) {
                return Ok(Some(path.clone()));
            }

            carried = overlap.min(filled);
            buf.copy_within(filled - carried..filled, 0);
        }
    }

    Ok(None)
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

                publish_archive(&response_data, &artifact_archive_path).await?;
            }
        }
    }

    if !artifact_archive_path.exists() {
        return Err(Status::not_found("artifact archive not found"));
    }

    send_message(format!("unpack artifact: {artifact_digest}"), tx).await?;

    publish_unpacked(&artifact_archive_path, &artifact_output_path).await
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

/// The environment a step observes for the artifact it is building.
///
/// `artifact_path` is wherever the caller is physically building right now — a
/// private staging directory during a build, never the shared store path,
/// which only comes to exist via the publish rename once every step has
/// finished. A step's reference to its own digest must therefore be that same
/// path: asking `get_artifact_output_path` for it would hand the step a path
/// that does not exist yet, and would disagree with `VORPAL_OUTPUT` — two
/// different answers for one physical location.
fn output_environments(
    artifact_digest: &str,
    artifact_path: &Path,
    workspace_path: &Path,
) -> Vec<String> {
    vec![
        format!(
            "VORPAL_ARTIFACT_{}={}",
            artifact_digest,
            artifact_path.display()
        ),
        format!("VORPAL_OUTPUT={}", artifact_path.display()),
        format!("VORPAL_WORKSPACE={}", workspace_path.display()),
    ]
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

    environments.extend(output_environments(
        artifact_digest,
        artifact_path,
        workspace_path,
    ));

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
/// the registry, and stores the artifact record. Called for every build whose staged
/// output passed `stage_then_publish`'s empty-output and embedded-path checks.
///
/// Runs inside the `stage_then_publish` closure in `build_artifact`: every error here
/// propagates via `?` rather than discarding the staging directory itself, since
/// `stage_then_publish` discards it centrally on any `Err` from the closure.
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is independent context (paths, digests, auth headers, request payload) threaded through from build_artifact; grouping them would need a bespoke struct with no reuse beyond this call site"
)]
async fn pack_push_and_store_artifact(
    artifact: Artifact,
    artifact_digest: &str,
    artifact_staging_path: &std::path::PathBuf,
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
        set_timestamps(path).await.map_err(|err| {
            error!("worker |> failed to sanitize output files: {:?}", err);
            Status::internal(format!("failed to sanitize output files: {err:?}"))
        })?;
    }

    // Create archive

    let artifact_archive = create_sandbox_file(Some("tar.zst"))
        .await
        .map_err(|err| Status::internal(format!("failed to create artifact archive: {err}")))?;

    compress_zstd(artifact_staging_path, artifact_path_files, &artifact_archive)
        .await
        .map_err(|err| {
            error!("worker |> failed to compress artifact: {:?}", err);
            Status::internal(format!("failed to compress artifact: {err:?}"))
        })?;

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

    remove_file(&artifact_archive).await.map_err(|err| {
        error!("worker |> failed to remove artifact archive: {:?}", err);
        Status::internal(format!("failed to remove artifact archive: {err:?}"))
    })?;

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

/// Releases what a failed build holds for its digest: the workspace it ran in
/// and the lock every later build of the same digest trips over.
///
/// Failures are logged rather than returned, because the caller is already
/// reporting the failure that brought it here and replacing that message with
/// a cleanup error would hide it.
async fn release_build(workspace_path: &Path, lock_path: &Path) {
    if let Err(err) = remove_dir_all(workspace_path).await {
        error!("worker |> failed to remove workspace: {:?}", err);
    }

    if let Err(err) = remove_file(lock_path).await {
        error!("worker |> failed to remove lock file: {:?}", err);
    }
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

    // Run steps, then publish
    //
    // Everything a build writes goes into the private staging directory
    // `stage_then_publish` hands the closure below; `artifact_output_path`
    // comes into existence only in that helper's publish rename. A crash, a
    // kill, or any failure inside the closure therefore strands the staging
    // directory alone, and the bare `exists()` readers of the real path —
    // `pull_artifact`, `run_step`'s dependency gate, the already-exists check
    // above — cannot observe this build until it is complete.
    //
    // `tx` and the workspace are re-bound as references so the `move` closure
    // copies them: the tail of this function still reports through `tx` and
    // still has to clean the workspace up.
    let tx = &tx;
    let workspace = workspace_path.as_path();
    let artifact_aliases = request.artifact_aliases;
    let store_namespace = artifact_namespace.clone();

    let published = stage_then_publish(&artifact_output_path, move |artifact_staging_path| async move {
        for step in artifact.steps.iter() {
            run_step(
                artifact_digest,
                artifact_namespace,
                &artifact_staging_path,
                step.clone(),
                tx,
                workspace,
            )
            .await
            .map_err(|err| {
                error!("worker |> failed to run step: {:?}", err);
                Status::internal(err.message())
            })?;
        }

        let staged = staged_entries(&artifact_staging_path)?;
        let staged_paths: Vec<PathBuf> = staged
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect();
        let staged_files = staged_content_paths(&staged);

        // Decide here that there is nothing to publish, before scanning or
        // packing a build that cannot ship. `stage_then_publish` makes the
        // same call as the backstop for every other producer.
        if staged_files.is_empty() {
            return Err(Status::internal("artifact produced no output files"));
        }

        // Refuse to publish output that records where it was built.
        //
        // Steps observe the staging directory through `VORPAL_OUTPUT` and
        // `VORPAL_ARTIFACT_<digest>`, and the publish rename deletes that
        // directory. Anything that wrote the path into what it produced would
        // be published — and pushed to the registry for every other worker —
        // referring to a directory that no longer exists, inside a namespace
        // directory a local user can create entries in. Making the observed
        // path equal the published one needs a bind mount, which is not
        // cheaply available on darwin, so the honest answer here is to fail
        // rather than ship the artifact broken.
        //
        // Only the staging root is scanned for. `VORPAL_WORKSPACE` is equally
        // ephemeral and is not, because nothing has measured what would stop
        // building if it were.
        let staging_name = artifact_staging_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("staging_path_for names every staging directory `.tmp-<uuid>`");

        if let Some(offender) = find_embedded_reference(&staged_files, staging_name).await? {
            let offender = offender
                .strip_prefix(&artifact_staging_path)
                .unwrap_or(&offender)
                .display()
                .to_string();

            error!("worker |> artifact embeds its build path: {}", offender);

            return Err(Status::internal(format!(
                "artifact embeds its build-time output path in {offender}, which does not survive publishing"
            )));
        }

        pack_push_and_store_artifact(
            artifact,
            artifact_digest,
            &artifact_staging_path,
            &staged_paths,
            archive_auth_header.as_ref(),
            artifact_auth_header.as_ref(),
            &registry,
            artifact_aliases,
            store_namespace,
            tx,
        )
        .await?;

        Ok(())
    })
    .await;

    // A build that refuses to publish — one that produced no files, or one
    // whose output records the directory it was built in — must leave the
    // digest exactly as buildable as it was. The lock is what the next build
    // of this digest trips over, and digests are recipe-addressed, so a
    // stranded lock denies that recipe until someone deletes the file by hand.
    let published = match published {
        Ok(published) => published,
        Err(err) => {
            release_build(&workspace_path, &artifact_output_lock).await;

            return Err(err);
        }
    };

    // Losing the publish race is not a failure, but this build already pushed
    // its own archive to the registry, and the store now holds someone else's
    // bytes for this digest. Nothing verifies the two agree, so tell the
    // client that pushed those bytes rather than reporting a plain success.
    if published == PublishOutcome::Superseded {
        info!(
            "worker |> published concurrently by another builder: {}",
            artifact_digest
        );

        if let Err(err) = send_message(format!("superseded: {artifact_digest}"), tx).await {
            release_build(&workspace_path, &artifact_output_lock).await;

            return Err(err);
        }
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
    use super::*;
    use std::{
        collections::BTreeSet,
        os::unix::fs::MetadataExt,
        sync::{Arc, Mutex},
    };
    use tempfile::TempDir;

    fn write_files(dir: &Path, names: &[&str], contents: &str) {
        for name in names {
            std::fs::write(dir.join(name), contents).unwrap();
        }
    }

    fn dir_entry_names(dir: &Path) -> BTreeSet<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    // AC2, archive half, pinned at the call site rather than at the seam: the
    // pulled archive is staged elsewhere and published onto its shared path, so
    // a file already there is replaced whole — a different inode carrying the
    // new bytes — never truncated in place under a reader's open handle. The
    // distinct payloads are what tells a replace from a discard.
    #[tokio::test]
    async fn publish_archive_replaces_the_real_path_instead_of_writing_into_it() {
        let root = TempDir::new().unwrap();
        let archive_dir = root.path().join("archives");
        let archive_path = archive_dir.join("abc123.tar.zst");

        std::fs::create_dir_all(&archive_dir).unwrap();
        std::fs::write(&archive_path, b"sentinel-bytes").unwrap();

        let sentinel_inode = std::fs::metadata(&archive_path).unwrap().ino();

        publish_archive(b"first-bytes", &archive_path)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&archive_path).unwrap(), b"first-bytes");
        assert_ne!(
            std::fs::metadata(&archive_path).unwrap().ino(),
            sentinel_inode,
            "the archive was written into the real path instead of published onto it"
        );

        publish_archive(b"second-bytes", &archive_path)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&archive_path).unwrap(), b"second-bytes");
        assert_eq!(
            dir_entry_names(&archive_dir),
            BTreeSet::from(["abc123.tar.zst".to_string()]),
            "a staging file was left under the store directory"
        );
    }

    // The publish step's failure path discards the staged copy twice — once
    // inside publish_atomically, once at the wrapper's error exit, where it
    // finds nothing left. The caller must still be told the publish failed, and
    // no staged archive may survive under the store directory.
    #[tokio::test]
    async fn publish_archive_reports_a_failed_publish_and_leaves_no_staging() {
        let root = TempDir::new().unwrap();
        let archive_dir = root.path().join("archives");
        let archive_path = archive_dir.join("abc123.tar.zst");

        std::fs::create_dir_all(&archive_path).unwrap();
        write_files(&archive_path, &["occupied.txt"], "occupied");

        let err = publish_archive(b"archive-bytes", &archive_path)
            .await
            .unwrap_err();

        assert!(err.message().contains("failed to publish"), "{err:?}");
        assert_eq!(
            dir_entry_names(&archive_dir),
            BTreeSet::from(["abc123.tar.zst".to_string()]),
            "a staged archive survived a failed publish"
        );
        assert_eq!(
            dir_entry_names(&archive_path),
            BTreeSet::from(["occupied.txt".to_string()])
        );
    }

    // AC1 at the call site: publish_unpacked must never unpack into the real
    // output path. A dependency already published there survives a pull whose
    // archive turns out to be garbage, byte for byte and inode for inode — an
    // in-place unpack would create into that path and then delete it while
    // cleaning up.
    #[tokio::test]
    async fn a_failed_unpack_leaves_an_already_published_output_path_untouched() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");
        let output_path = store_path.join("abc123");

        std::fs::create_dir_all(&output_path).unwrap();
        write_files(&output_path, &["published.txt"], "published-content");

        let published_inode = std::fs::metadata(&output_path).unwrap().ino();
        let archive_path = root.path().join("abc123.tar.zst");

        std::fs::write(&archive_path, "not a zstd archive").unwrap();

        publish_unpacked(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert_eq!(
            dir_entry_names(&output_path),
            BTreeSet::from(["published.txt".to_string()]),
            "a failed unpack disturbed an already published output path"
        );
        assert_eq!(
            std::fs::read_to_string(output_path.join("published.txt")).unwrap(),
            "published-content"
        );
        assert_eq!(
            std::fs::metadata(&output_path).unwrap().ino(),
            published_inode
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "a failed unpack left its staging directory under the store"
        );
    }

    // AC2, kill case, driven through the real unpack path: an unpack that dies
    // partway must leave nothing at the real output path, so the exists() check
    // a later pull performs can never mistake wreckage for a cache hit — and it
    // must not leave its staging directory under the store either.
    #[tokio::test]
    async fn a_failed_unpack_leaves_the_real_output_path_absent() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        std::fs::write(&archive_path, "not a zstd archive").unwrap();

        let output_path = store_path.join("abc123");

        publish_unpacked(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            !output_path.exists(),
            "a failed unpack left debris at the real output path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "a failed unpack left its staging directory under the store"
        );
    }

    fn store_dir() -> (TempDir, PathBuf, PathBuf) {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let output_path = store_path.join("abc123");

        (root, store_path, output_path)
    }

    // The conversion this seam exists for: a build never writes at the real
    // output path, and the real path is created by moving the finished
    // staging directory onto it. Producing in place would satisfy the inode
    // assertion on its own and observing a different directory would satisfy
    // the identity assertion on its own — together they pin the rename.
    #[tokio::test]
    async fn a_finished_producer_is_published_by_renaming_its_staging_directory() {
        let (_root, store_path, output_path) = store_dir();
        let staged_inode = Arc::new(Mutex::new(None));

        let recorder = Arc::clone(&staged_inode);
        let real_path = output_path.clone();

        let outcome = stage_then_publish(&output_path, move |staging_path| async move {
            assert_ne!(
                staging_path, real_path,
                "the producer was handed the real output path to build in"
            );
            assert!(
                !real_path.exists(),
                "the real output path existed while the producer was still building"
            );

            std::fs::write(staging_path.join("built.txt"), "built").unwrap();

            *recorder.lock().unwrap() = Some(std::fs::metadata(&staging_path).unwrap().ino());

            Ok(())
        })
        .await
        .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "a staging directory was left under the store"
        );
        assert_eq!(
            std::fs::read_to_string(output_path.join("built.txt")).unwrap(),
            "built"
        );
        assert_eq!(
            std::fs::metadata(&output_path).unwrap().ino(),
            staged_inode.lock().unwrap().unwrap(),
            "the staged content was copied into the real path instead of renamed onto it"
        );
    }

    // A build that dies partway — a failing step, a compress or push failure —
    // must leave the digest exactly as buildable as it was. Every reader tests
    // readiness with a bare `exists()`, so half-written output surviving at the
    // real path would be taken for a finished artifact forever.
    #[tokio::test]
    async fn a_failing_producer_leaves_the_real_output_path_absent() {
        let (_root, store_path, output_path) = store_dir();

        let err = stage_then_publish(&output_path, |staging_path| async move {
            std::fs::write(staging_path.join("half-written.txt"), "half").unwrap();

            Err(Status::internal("step failed"))
        })
        .await
        .unwrap_err();

        assert_eq!(err.message(), "step failed");
        assert!(
            !output_path.exists(),
            "a failed build left debris at the real output path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "a failed build left its staging directory under the store"
        );
    }

    // An empty directory is indistinguishable from a complete artifact to
    // every `exists()` reader, and a rename publishes one perfectly happily.
    // A producer that wrote nothing must therefore not reach the store at all:
    // caching emptiness at a digest denies that digest to every later build.
    #[tokio::test]
    async fn a_producer_that_wrote_nothing_publishes_nothing() {
        let (_root, store_path, output_path) = store_dir();

        let err = stage_then_publish(
            &output_path,
            |_staging_path| async move { Ok::<(), Status>(()) },
        )
        .await
        .unwrap_err();

        assert!(err.message().contains("no output files"), "{err:?}");
        assert!(
            !output_path.exists(),
            "an empty build was published onto the shared store path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "an empty build left its staging directory under the store"
        );
    }

    // Directories carry no bytes. A producer whose whole output is
    // `mkdir -p $VORPAL_OUTPUT/bin` — which artifacts in this repository do —
    // has produced nothing a dependent can use, and every `exists()` reader
    // would take the published tree for a finished artifact forever.
    #[tokio::test]
    async fn a_producer_that_made_only_directories_publishes_nothing() {
        let (_root, store_path, output_path) = store_dir();

        let err = stage_then_publish(&output_path, |staging_path| async move {
            std::fs::create_dir_all(staging_path.join("bin")).unwrap();

            Ok::<(), Status>(())
        })
        .await
        .unwrap_err();

        assert!(err.message().contains("no output files"), "{err:?}");
        assert!(
            !output_path.exists(),
            "a build that produced only directories was published onto the shared store path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "a fileless build left its staging directory under the store"
        );
    }

    // The producer owns the staging directory for the whole of its run and can
    // replace it with a symlink. Reads follow symlinks and the publish rename
    // does not, so a check made through the link would describe one tree while
    // another — the link itself — is installed as the store entry.
    #[tokio::test]
    async fn a_producer_that_swapped_its_staging_directory_for_a_symlink_publishes_nothing() {
        let (root, store_path, output_path) = store_dir();
        let elsewhere = root.path().join("elsewhere");

        std::fs::create_dir_all(&elsewhere).unwrap();
        write_files(&elsewhere, &["borrowed.txt"], "borrowed");

        let target = elsewhere.clone();

        let err = stage_then_publish(&output_path, move |staging_path| async move {
            std::fs::remove_dir(&staging_path).unwrap();
            std::os::unix::fs::symlink(&target, &staging_path).unwrap();

            Ok::<(), Status>(())
        })
        .await
        .unwrap_err();

        assert!(err.message().contains("no longer a directory"), "{err:?}");
        assert!(
            !output_path.exists(),
            "a step-controlled symlink was published as the store entry"
        );
        assert_eq!(
            dir_entry_names(&elsewhere),
            BTreeSet::from(["borrowed.txt".to_string()]),
            "the symlink's target was disturbed"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "the staging symlink survived under the store"
        );
    }

    // Two builders can stage one digest at once, and the loser has already
    // pushed its own archive by the time it publishes. It must be told it lost
    // rather than handed the same success the winner gets.
    #[tokio::test]
    async fn a_producer_that_lost_the_publish_race_is_told_it_lost() {
        let (_root, store_path, output_path) = store_dir();

        std::fs::create_dir_all(&output_path).unwrap();
        write_files(&output_path, &["winner.txt"], "winner");

        let outcome = stage_then_publish(&output_path, |staging_path| async move {
            std::fs::write(staging_path.join("loser.txt"), "loser").unwrap();

            Ok::<(), Status>(())
        })
        .await
        .unwrap();

        assert_eq!(outcome, PublishOutcome::Superseded);
        assert_eq!(
            dir_entry_names(&output_path),
            BTreeSet::from(["winner.txt".to_string()]),
            "the loser overwrote the winner's published entry"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "the loser's staging directory survived under the store"
        );
    }

    // The embedded-path scan and the packing list are both computed from this
    // walk, and the publish renames the whole staged tree. An entry the walk
    // drops is published unscanned and is absent from the archive pushed for
    // the same digest; `.git` is what the packing walker drops, and a
    // submodule checkout puts an absolute build path inside `.git`.
    #[test]
    fn staged_entries_reports_the_dot_git_entries_the_packing_walker_drops() {
        let root = TempDir::new().unwrap();
        let staged = root.path().join("staged");
        let git_dir = staged.join(".git");

        std::fs::create_dir_all(&git_dir).unwrap();
        write_files(
            &git_dir,
            &["config"],
            "gitdir: /store/output/default/.tmp-uuid",
        );
        write_files(&staged, &["a.txt"], "a");

        let relative: BTreeSet<String> = staged_entries(&staged)
            .unwrap()
            .iter()
            .map(|entry| {
                entry
                    .path()
                    .strip_prefix(&staged)
                    .unwrap()
                    .display()
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect();

        assert_eq!(
            relative,
            BTreeSet::from([
                ".git".to_string(),
                ".git/config".to_string(),
                "a.txt".to_string()
            ])
        );
        assert_eq!(
            get_file_paths(&staged, vec![], vec![]).unwrap().len(),
            2,
            "the packing walker stopped dropping .git, so this test no longer pins anything"
        );
    }

    // Output that records the directory it was built in cannot be published:
    // the publish rename deletes that directory. The reference is found
    // wherever it sits, including straddling the boundary between two reads of
    // a file far larger than one buffer.
    #[tokio::test]
    async fn find_embedded_reference_finds_a_reference_across_a_read_boundary() {
        let root = TempDir::new().unwrap();
        let needle = ".tmp-0199b0f0-0000-7000-8000-000000000000";
        let straddling = root.path().join("binary");

        // The scan reads SCAN_CHUNK_SIZE + needle.len() - 1 bytes at a time,
        // so straddle that boundary: half the reference lands in one read and
        // half in the next, and only the carried overlap can see it whole.
        let boundary = SCAN_CHUNK_SIZE + needle.len() - 1;
        let mut contents = vec![b'.'; boundary - (needle.len() / 2)];

        contents.extend_from_slice(needle.as_bytes());
        contents.resize(boundary * 2, b'.');

        std::fs::write(&straddling, &contents).unwrap();

        let found = find_embedded_reference(std::slice::from_ref(&straddling), needle)
            .await
            .unwrap();

        assert_eq!(found, Some(straddling));
    }

    // A symlink records its target without any of its bytes appearing in a
    // file, so the target is where it has to be read from.
    #[tokio::test]
    async fn find_embedded_reference_reads_symlink_targets() {
        let root = TempDir::new().unwrap();
        let needle = ".tmp-0199b0f0-0000-7000-8000-000000000000";
        let link = root.path().join("link");

        std::os::unix::fs::symlink(root.path().join(needle).join("lib"), &link).unwrap();

        assert_eq!(
            find_embedded_reference(std::slice::from_ref(&link), needle)
                .await
                .unwrap(),
            Some(link)
        );
    }

    // The negative control: ordinary output that merely mentions the store,
    // or the same UUID shape under a different name, must publish normally.
    #[tokio::test]
    async fn find_embedded_reference_ignores_output_without_the_reference() {
        let root = TempDir::new().unwrap();
        let needle = ".tmp-0199b0f0-0000-7000-8000-000000000000";
        let clean = root.path().join("clean");

        std::fs::write(
            &clean,
            "/var/lib/vorpal/store/artifact/output/default/abc123/bin/tool",
        )
        .unwrap();

        assert_eq!(
            find_embedded_reference(&[clean], needle).await.unwrap(),
            None
        );
    }

    // A step reading `VORPAL_OUTPUT` and a step reading its own digest's
    // variable are asking the same question, and a build answers it with the
    // staging directory. Two different answers would silently publish an
    // artifact built half against a path that does not exist yet.
    #[test]
    fn a_step_sees_one_path_for_its_own_output() {
        let environments = output_environments(
            "abc123",
            Path::new("/store/output/default/.tmp-uuid"),
            Path::new("/workspace"),
        );

        let value_of = |key: &str| {
            environments
                .iter()
                .find_map(|e| e.strip_prefix(&format!("{key}=")))
                .map(str::to_string)
                .unwrap_or_else(|| panic!("{key} missing from {environments:?}"))
        };

        assert_eq!(value_of("VORPAL_OUTPUT"), "/store/output/default/.tmp-uuid");
        assert_eq!(
            value_of("VORPAL_ARTIFACT_abc123"),
            value_of("VORPAL_OUTPUT")
        );
        assert_eq!(value_of("VORPAL_WORKSPACE"), "/workspace");
    }
}
