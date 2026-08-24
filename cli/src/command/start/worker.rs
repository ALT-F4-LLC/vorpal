use crate::command::{
    start::auth,
    store::{
        archives::{compress_zstd, unpack_zstd},
        notary,
        paths::{
            discard_staging, get_artifact_archive_path, get_artifact_output_lock_path,
            get_artifact_output_path, get_file_paths, get_key_service_key_path,
            parse_artifact_digest, parse_store_path_component, publish_atomically, set_timestamps,
            staging_path_for, PublishOutcome,
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
    Request, Response, Status, Streaming,
};
use tracing::{error, info};
use vorpal_sdk::{
    api::{
        archive::{
            archive_service_client::ArchiveServiceClient, ArchivePullRequest,
            ArchivePullResponse, ArchivePushRequest,
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
    /// The operator-configured registry allow-list. A `BuildArtifactRequest`
    /// may only select a registry from this set (or leave it unset to get
    /// the worker's own configured value); see `resolve_registry`. Populated
    /// from `--registry-allowed` (or `VORPAL_REGISTRY_ALLOWED`) in
    /// `cli/src/command.rs`.
    pub registry_allowed: Vec<String>,
}

impl WorkerServer {
    pub fn new(
        issuer: Option<String>,
        issuer_audience: Option<String>,
        issuer_client_id: Option<String>,
        issuer_client_secret: Option<String>,
        registry_allowed: Vec<String>,
    ) -> Self {
        Self {
            issuer_audience,
            issuer_client_id,
            issuer_client_secret,
            issuer,
            registry_allowed,
        }
    }
}

// `resolve_registry`/`ResolvedRegistry` live in `cli/src/command/start.rs`
// (the shared parent of this module and `agent`), not here — both the
// worker's `build_artifact` and the agent's `prepare_artifact` dial a
// caller-named registry, so both need the identical check, and a shared
// parent is where a policy two siblings depend on belongs, rather than one
// sibling importing it sideways from the other.
use super::{resolve_registry, ResolvedRegistry};

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

/// One side of a registry pull: the sequence of chunks and the status that
/// ends it. `tonic::Streaming` is the production source; tests hand-feed a
/// scripted sequence so the truncated-publish fix (threat model C3) is
/// testable without a real registry connection.
trait ChunkSource {
    async fn next_chunk(&mut self) -> Result<Option<ArchivePullResponse>, Status>;
}

impl ChunkSource for Streaming<ArchivePullResponse> {
    async fn next_chunk(&mut self) -> Result<Option<ArchivePullResponse>, Status> {
        self.message().await
    }
}

/// Accumulates one registry pull stream into memory. Shared by `pull_source`
/// and `pull_artifact`, which were previously two copies of this same loop.
///
/// Explicit match, not `while let Ok(..)`: that pattern treats a stream
/// `Err` the same as a clean end-of-stream, so a connection drop
/// mid-transfer fell through to the caller's `publish_archive` with only the
/// bytes received so far — caching a truncated archive under the digest
/// (threat model C3). Returning `Err` here instead means the caller's
/// `response_data` is discarded by never being produced.
///
/// `NotFound` before any byte arrived is the same "the registry does not
/// have it" disposition as an RPC-initiation `NotFound` — not an internal
/// worker fault (threat model C3's reviewer checklist item 5; mirrors
/// `cli/src/command/build.rs`'s `publish_archive_stream`) — so it ends the
/// loop with whatever was accumulated (nothing), leaving the caller's own
/// empty-check to answer it the same way that check already does.
/// `NotFound` after bytes arrived, or any other error, is a genuine
/// transfer failure: the full `Status` (transport/metadata detail) is
/// logged server-side only, never returned to the client.
async fn accumulate_archive_stream(
    source: &mut impl ChunkSource,
    error_context: &str,
) -> Result<Vec<u8>, Status> {
    let mut response_data = Vec::new();

    loop {
        match source.next_chunk().await {
            Ok(Some(res)) => {
                if !res.data.is_empty() {
                    response_data.extend(res.data);
                }
            }
            Ok(None) => break,
            Err(status) => {
                if status.code() == NotFound && response_data.is_empty() {
                    break;
                }

                error!(
                    "worker |> {error_context} stream failed after {} bytes: {status:?}",
                    response_data.len()
                );

                return Err(Status::internal(format!(
                    "{error_context} stream failed before completion"
                )));
            }
        }
    }

    Ok(response_data)
}

async fn pull_source(
    archive_auth_header: Option<MetadataValue<Ascii>>,
    artifact_namespace: String,
    artifact_source: &ArtifactSource,
    artifact_source_dir_path: &Path,
    registry: ResolvedRegistry,
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

                let response_data =
                    accumulate_archive_stream(&mut response, "source archive").await?;

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

/// Every entry staged under `staging_path`, the root included, ordered by
/// path bytes.
///
/// Walked here rather than through `get_file_paths`: that walker builds a
/// packing list, so it drops `.git` entries and turns a walk error into a
/// silently missing subtree. The publish renames the whole staging directory,
/// so an entry missing from this list is published unscanned and is absent
/// from the archive pushed for the same digest — two workers would then hold
/// different content under one digest.
///
/// The order is the walker's only externally visible property and it is fixed
/// here rather than inherited: raw `WalkDir` order is `read_dir` order, which
/// differs between filesystems and with creation order, and this list is what
/// tar entries are appended in. Sorting by path bytes — the same normalization
/// `get_file_paths` ends with — is what lets two workers building one recipe
/// push byte-identical archives. A parent path is a prefix of its children, so
/// the sort keeps every directory ahead of its contents and extraction order
/// is unaffected.
fn staged_entries(staging_path: &Path) -> Result<Vec<DirEntry>, Status> {
    let mut entries = WalkDir::new(staging_path)
        .into_iter()
        .map(|entry| {
            entry.map_err(|err| Status::internal(format!("failed to read staged output: {err}")))
        })
        .collect::<Result<Vec<DirEntry>, Status>>()?;

    entries.sort_by(|a, b| a.path().cmp(b.path()));

    Ok(entries)
}

/// Refuses a staged root that is no longer a real directory.
///
/// The producer owns the staging path for the whole of its run and can replace
/// it with a symlink. Reads follow symlinks and the publish rename does not, so
/// a check made through the link describes one tree while the link itself is
/// what gets installed as the store entry.
async fn ensure_staged_directory(staging_path: &Path) -> Result<(), Status> {
    let metadata = symlink_metadata(staging_path)
        .await
        .map_err(|err| Status::internal(format!("failed to stat staged output: {err}")))?;

    if !metadata.is_dir() {
        return Err(Status::internal("staged output is no longer a directory"));
    }

    Ok(())
}

/// Whether anything under `staging_path` carries an artifact's bytes.
///
/// Stops at the first regular file or symlink: this is the seam's backstop, and
/// both producers have already walked their finished tree in full by the time
/// it runs, so a walk error anywhere in that tree has already been reported.
fn staged_has_content(staging_path: &Path) -> Result<bool, Status> {
    for entry in WalkDir::new(staging_path) {
        let entry = entry
            .map_err(|err| Status::internal(format!("failed to read staged output: {err}")))?;

        if entry.file_type().is_file() || entry.file_type().is_symlink() {
            return Ok(true);
        }
    }

    Ok(false)
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

        ensure_staged_directory(&staging_path).await?;

        if !staged_has_content(&staging_path)? {
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
    registry: &ResolvedRegistry,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    // Both values are joined into store paths below. `build_artifact` parses
    // them at the request boundary; parsing them again here costs two string
    // scans and keeps the guarantee with the function that does the joining,
    // rather than with whoever calls it.
    parse_store_path_component(artifact_namespace, "namespace")
        .map_err(|err| Status::invalid_argument(err.to_string()))?;

    parse_artifact_digest(artifact_digest, "artifact dependency")
        .map_err(|err| Status::invalid_argument(err.to_string()))?;

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

                let response_data =
                    accumulate_archive_stream(&mut response, "artifact archive").await?;

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

    // A refused publish is a statement about these bytes — they unpacked to no
    // files, or to a tree that cannot be read. The archive is cached before it
    // is unpacked and the download is skipped whenever the cache file exists,
    // so leaving it in place makes the refusal permanent: every later pull
    // re-unpacks the same bytes and fails identically, and the digest, plus
    // every build depending on it, is denied until someone deletes the file by
    // hand. Dropping the cache entry turns that into a retry against the
    // registry.
    if let Err(err) = publish_unpacked(&artifact_archive_path, &artifact_output_path).await {
        if let Err(remove_err) = remove_file(&artifact_archive_path).await {
            error!(
                "worker |> failed to discard unusable artifact archive: {:?}",
                remove_err
            );
        }

        return Err(err);
    }

    Ok(())
}

/// Substitutes `KEY=VALUE` entries into `text`, in both `${KEY}` and `$KEY`
/// spellings.
///
/// `step.environments` is a free-form list of request strings, so an entry
/// carrying no `=` reaches here; it names no variable and is skipped rather
/// than indexed. The value is everything after the first `=`, so a value
/// containing `=` survives intact.
fn expand_env(text: &str, envs: &[&String]) -> String {
    envs.iter().fold(text.to_string(), |acc, e| {
        let Some((key, value)) = e.split_once('=') else {
            return acc;
        };

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

    // A request's `step.environments` entries arrive unvalidated, so one
    // without a `=` is an ordinary bad request rather than an invariant
    // violation: refuse it here instead of indexing past the end of a split.
    // The offending value is not echoed — secrets are carried in this same
    // list, and the client already knows what it sent.
    for env in environments_sorted.iter() {
        let Some((key, value)) = env.split_once('=') else {
            return Err(Status::invalid_argument(
                "step environment entry is not 'KEY=VALUE'",
            ));
        };

        command.env(key, expand_env(value, &vorpal_envs));
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

/// Releases what a *failed* build holds for its digest: the workspace it ran in
/// and the lock every later build of the same digest trips over.
///
/// This is one half of a pair — the success path at the end of `build_artifact`
/// removes the same two things. The halves are deliberately not shared: here a
/// cleanup failure is logged, because the caller is already reporting the
/// failure that brought it here and replacing that message with a cleanup error
/// would hide it, while on the success path a cleanup failure is the only
/// failure there is and is reported to the client.
async fn release_failed_build(workspace_path: &Path, lock_path: &Path) {
    if let Err(err) = remove_dir_all(workspace_path).await {
        error!("worker |> failed to remove workspace: {:?}", err);
    }

    if let Err(err) = remove_file(lock_path).await {
        error!("worker |> failed to remove lock file: {:?}", err);
    }
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

    for step in artifact.steps.iter() {
        for step_artifact in step.artifacts.iter() {
            parse_artifact_digest(step_artifact, "artifact step")
                .map_err(|err| Status::invalid_argument(err.to_string()))?;
        }
    }

    for artifact_source in artifact.sources.iter() {
        if let Some(source_digest) = artifact_source.digest.as_ref() {
            parse_artifact_digest(source_digest, "artifact source")
                .map_err(|err| Status::invalid_argument(err.to_string()))?;
        }

        parse_store_path_component(&artifact_source.name, "source name")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
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
    registry_allowed: &[String],
    request: BuildArtifactRequest,
    tx: &Sender<Result<BuildArtifactResponse, Status>>,
) -> Result<(), Status> {
    let artifact = request
        .artifact
        .ok_or_else(|| Status::invalid_argument("artifact is missing"))?;

    // The namespace and every digest inside the recipe are request strings that
    // this function, `pull_source`, `pull_artifact` and `run_step` all join
    // straight into store paths. Parse their shapes here, before the first path
    // is composed, so a value that would name a destination outside the store is
    // refused as a bad request rather than resolved against the filesystem.
    let artifact_namespace = parse_store_path_component(&request.artifact_namespace, "namespace")
        .map_err(|err| Status::invalid_argument(err.to_string()))?;

    let artifact_namespace = &artifact_namespace;

    // The registry is the one request field on this path with no allow-list
    // check until now — resolve it before the target check and, critically,
    // before `obtain_service_credentials` below, which performs its own
    // network round trip to the issuer. A request naming a registry outside
    // the operator's configured set is refused here, before any network I/O.
    let registry = resolve_registry(&request.registry, registry_allowed)?;

    info!("worker |> resolved registry: {}", registry);

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
    //
    // The lock now exists, and digests are recipe-addressed, so a lock left
    // behind refuses every later build of this recipe until someone deletes the
    // file by hand. From here on every failure has to release it — including
    // this one, which has no workspace to release yet.
    let workspace_path = match create_sandbox_dir().await {
        Ok(path) => path,
        Err(err) => {
            if let Err(err) = remove_file(&artifact_output_lock).await {
                error!("worker |> failed to remove lock file: {:?}", err);
            }

            return Err(Status::internal(format!(
                "failed to create workspace: {err}"
            )));
        }
    };

    let built: Result<(), Status> = async {
        let artifact_source_dir_path = workspace_path.join("source");

        if let Err(err) = create_dir_all(&artifact_source_dir_path).await {
            error!("worker |> failed to create source path: {:?}", err);
            return Err(Status::internal(format!(
                "failed to create source path: {err:?}"
            )));
        }

        // Pull sources

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

        // Pull dependency artifacts

        let mut dependency_digests = HashSet::new();
        for step in artifact.steps.iter() {
            for dep_digest in step.artifacts.iter() {
                dependency_digests.insert(dep_digest.clone());
            }
        }

        for dep_digest in dependency_digests.iter() {
            pull_artifact(
                archive_auth_header.as_ref(),
                artifact_namespace,
                dep_digest,
                &registry,
                &tx,
            )
            .await?;

            info!("worker |> pull artifact: {}", dep_digest);
        }

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
        // `tx`, the workspace and the recipe's steps are re-bound as references so
        // the `move` closure copies those rather than the values: the tail of this
        // function still reports through `tx`, still has to clean the workspace
        // up, and still has to register `artifact` with the registry.
        let tx = &tx;
        let workspace = workspace_path.as_path();
        let artifact_steps = &artifact.steps;
        let artifact_aliases = request.artifact_aliases;
        let store_namespace = artifact_namespace.clone();

        let published = stage_then_publish(&artifact_output_path, move |artifact_staging_path| async move {
            for step in artifact_steps.iter() {
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
            let staged_files = staged_content_paths(&staged);

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
            // `staging_path_for` names every staging directory `.tmp-<uuid>`, so
            // this cannot fail today. It is still an error return rather than an
            // assertion: this closure's cleanup — the seam's `discard_staging` and
            // the caller's lock release — is written on the error path, and a panic
            // here would run neither, stranding both the lock and a staging
            // directory inside the shared namespace.
            let staging_name = artifact_staging_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| Status::internal("staging path has no name"))?;

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

            // Sanitize files
            //
            // Before the publish rather than after it: these are the bytes the
            // rename installs as the store entry, and the archive pushed for this
            // digest is packed from that entry afterwards.
            for entry in staged.iter() {
                set_timestamps(&entry.path().to_path_buf())
                    .await
                    .map_err(|err| {
                        error!("worker |> failed to sanitize output files: {:?}", err);
                        Status::internal(format!("failed to sanitize output files: {err:?}"))
                    })?;
            }

            Ok(())
        })
        .await?;

        // Losing the publish race is not a failure: the winner's bytes stand at
        // the digest and this build's staged copy is gone. Nothing this build
        // produced is in the store, so there is nothing for it to advertise —
        // pushing its archive now would point every other worker at bytes this
        // worker does not hold — and the registry is left to the winner.
        if published == PublishOutcome::Superseded {
            info!(
                "worker |> published concurrently by another builder: {}",
                artifact_digest
            );

            send_message(format!("superseded: {artifact_digest}"), tx).await?;

            return Ok(());
        }

        // Publish to the registry, after the local publish and never before it.
        //
        // A push and a `store_artifact` are effects nothing local can take back:
        // once they land, every other worker resolves this digest to these bytes.
        // The local publish is the last step that can still refuse this build or
        // lose it to another builder, so it goes first and the registry only ever
        // learns about a digest this worker really holds. The archive is packed
        // from the published path for the same reason — that is where the staged
        // content now lives, and it is what a puller of this digest will get.

        let published_entries = staged_entries(&artifact_output_path)?;

        let packing_paths: Vec<PathBuf> = published_entries
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect();

        send_message(format!("pack: {artifact_digest}"), tx).await?;

        // Create archive

        let artifact_archive = create_sandbox_file(Some("tar.zst"))
            .await
            .map_err(|err| Status::internal(format!("failed to create artifact archive: {err}")))?;

        compress_zstd(&artifact_output_path, &packing_paths, &artifact_archive)
            .await
            .map_err(|err| {
                error!("worker |> failed to compress artifact: {:?}", err);
                Status::internal(format!("failed to compress artifact: {err:?}"))
            })?;

        // TODO: check if archive is already uploaded

        // Upload archive

        // Create authenticated archive client for pushing
        let client_archive_channel = build_channel(&registry)
            .await
            .map_err(|err| Status::internal(format!("failed to connect to registry: {err}")))?;

        // Create client with authorization interceptor for pushing if token is available
        let mut client_archive = ArchiveServiceClient::with_interceptor(
            client_archive_channel,
            apply_auth_to_request(archive_auth_header.as_ref()),
        );

        send_message(format!("push: {artifact_digest}"), tx).await?;

        let artifact_file = File::open(&artifact_archive)
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

        client_archive.push(request_stream).await.map_err(|err| {
            error!("worker |> failed to push artifact: {:?}", err);
            Status::internal(format!("failed to push artifact: {err:?}"))
        })?;

        // Store artifact in registry

        // Create authenticated artifact client
        let client_artifact_channel = build_channel(&registry)
            .await
            .map_err(|err| Status::internal(format!("failed to connect to registry: {err}")))?;

        // Create client with authorization interceptor if token is available
        let mut client_artifact = ArtifactServiceClient::with_interceptor(
            client_artifact_channel,
            apply_auth_to_request(artifact_auth_header.as_ref()),
        );

        let store_request = StoreArtifactRequest {
            artifact: Some(artifact),
            artifact_aliases,
            artifact_namespace: store_namespace,
        };

        client_artifact
            .store_artifact(store_request)
            .await
            .map_err(|err| {
                Status::internal(format!("failed to store artifact in registry: {err}"))
            })?;

        // Remove artifact archive

        remove_file(&artifact_archive).await.map_err(|err| {
            error!("worker |> failed to remove artifact archive: {:?}", err);
            Status::internal(format!("failed to remove artifact archive: {err:?}"))
        })?;

        Ok(())
    }
    .await;

    // Everything above ran with the lock held: a source that would not pull, a
    // dependency this worker cannot publish, a step that failed, a build the
    // seam refused because it produced no files or recorded the directory it
    // was built in. Whichever it was, the digest must be left exactly as
    // buildable as it was.
    if let Err(err) = built {
        release_failed_build(&workspace_path, &artifact_output_lock).await;

        return Err(err);
    }

    // Remove workspace
    //
    // The success half of `release_failed_build`: the same two removals, but a
    // cleanup failure here is the only failure there is, so it is reported
    // rather than logged.

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
        let registry_allowed = self.registry_allowed.clone();

        tokio::spawn(async move {
            if let Err(err) = build_artifact(
                issuer.as_deref(),
                issuer_audience.as_deref(),
                issuer_client_id.as_deref(),
                issuer_client_secret.as_deref(),
                &registry_allowed,
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
    use vorpal_sdk::api::artifact::Artifact;

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

    fn relative_entry_names(root: &Path) -> Vec<String> {
        staged_entries(root)
            .unwrap()
            .iter()
            .map(|entry| {
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .display()
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect()
    }

    /// Writes a real zstd-compressed tar at `archive_path`. `entries` are
    /// relative paths; `None` contents means a directory entry.
    ///
    /// Built with the crate's own encoder and tar builder rather than through
    /// `compress_zstd`, which stages through the hardcoded `/var/lib/vorpal`
    /// sandbox directory and cannot run in a test process.
    async fn write_zstd_archive(archive_path: &Path, entries: &[(&str, Option<&str>)]) {
        use async_compression::tokio::write::ZstdEncoder;
        use tokio::io::AsyncWriteExt;
        use tokio_tar::Builder;

        let source = TempDir::new().unwrap();
        let file = File::create(archive_path).await.unwrap();
        let mut builder = Builder::new(ZstdEncoder::new(file));

        builder.follow_symlinks(false);

        for (name, contents) in entries {
            let path = source.path().join(name);

            match contents {
                Some(contents) => {
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(&path, contents).unwrap();
                }
                None => std::fs::create_dir_all(&path).unwrap(),
            }

            builder.append_path_with_name(&path, name).await.unwrap();
        }

        builder.finish().await.unwrap();

        let mut encoder = builder.into_inner().await.unwrap();

        encoder.shutdown().await.unwrap();
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

    /// A scripted `ChunkSource`, driven from a fixed sequence rather than a
    /// real connection — the fixture `accumulate_archive_stream`'s tests
    /// need to inject a stream error at a chosen point.
    struct ScriptedChunks {
        script: std::collections::VecDeque<Result<Option<ArchivePullResponse>, Status>>,
    }

    impl ScriptedChunks {
        fn new(script: Vec<Result<Option<ArchivePullResponse>, Status>>) -> Self {
            Self {
                script: script.into(),
            }
        }
    }

    impl ChunkSource for ScriptedChunks {
        async fn next_chunk(&mut self) -> Result<Option<ArchivePullResponse>, Status> {
            self.script
                .pop_front()
                .expect("accumulate_archive_stream read past the end of the scripted stream")
        }
    }

    fn chunk(data: &[u8]) -> Result<Option<ArchivePullResponse>, Status> {
        Ok(Some(ArchivePullResponse {
            data: data.to_vec(),
        }))
    }

    // C8 (reconcile): the truncated-publish fix had no failure-injection
    // test — reverting `accumulate_archive_stream`'s explicit match to the
    // pre-fix `while let Ok(..)` pattern (treating a stream `Err` as a clean
    // end-of-stream) would return `Ok(b"first-chunk")` here instead of
    // erroring, leaving this test the only thing that would catch it.
    #[tokio::test]
    async fn accumulate_archive_stream_discards_and_errors_on_a_stream_failure() {
        let mut source = ScriptedChunks::new(vec![
            chunk(b"first-chunk"),
            chunk(b"second-chunk"),
            Err(Status::internal("connection reset")),
        ]);

        let err = accumulate_archive_stream(&mut source, "test archive")
            .await
            .expect_err("a mid-stream failure must not return the partial bytes as success");

        assert_eq!(err.code(), tonic::Code::Internal);
        // The upstream `Status` debug rendering is logged, not returned —
        // the client-facing message names only the sanitized disposition.
        assert!(
            !err.message().contains("connection reset"),
            "the upstream status detail must not reach the client: {}",
            err.message()
        );
    }

    // Positive control: an uninterrupted stream publishes every chunk, in
    // order — proves the discard above is about the error, not about
    // `accumulate_archive_stream` losing bytes generally.
    #[tokio::test]
    async fn accumulate_archive_stream_returns_every_chunk_in_order_on_success() {
        let mut source =
            ScriptedChunks::new(vec![chunk(b"first-chunk"), chunk(b"second-chunk"), Ok(None)]);

        let data = accumulate_archive_stream(&mut source, "test archive")
            .await
            .expect("an uninterrupted stream must publish");

        assert_eq!(data, b"first-chunksecond-chunk".to_vec());
    }

    // C7 (reconcile): `NotFound` before any byte arrived is "the registry
    // does not have it", not an internal worker fault — the same
    // disposition as the RPC-initiation `NotFound` handled by the caller.
    #[tokio::test]
    async fn accumulate_archive_stream_treats_not_found_before_any_byte_as_empty() {
        let mut source = ScriptedChunks::new(vec![Err(Status::not_found("no such archive"))]);

        let data = accumulate_archive_stream(&mut source, "test archive")
            .await
            .expect("NotFound before any byte must not be an internal error");

        assert!(data.is_empty());
    }

    // NotFound *after* bytes arrived is a truncated transfer of an object
    // the registry did have, not an absent object — it must still discard
    // and error, exactly like any other mid-stream failure.
    #[tokio::test]
    async fn accumulate_archive_stream_errors_on_not_found_after_bytes_arrived() {
        let mut source = ScriptedChunks::new(vec![
            chunk(b"first-chunk"),
            Err(Status::not_found("stream ended early")),
        ]);

        let err = accumulate_archive_stream(&mut source, "test archive")
            .await
            .expect_err("NotFound after bytes arrived is a truncated transfer, not an absence");

        assert_eq!(err.code(), tonic::Code::Internal);
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

    // This list is the order tar entries are appended in, so it decides the
    // bytes of the archive pushed for a digest. Two workers building one recipe
    // must push the same bytes, and raw walk order is the host's `read_dir`
    // order — filesystem- and creation-order dependent.
    #[test]
    fn staged_entries_orders_entries_by_path_whatever_order_they_were_created_in() {
        let forward = TempDir::new().unwrap();
        let reversed = TempDir::new().unwrap();

        write_files(forward.path(), &["alpha", "bravo", "charlie"], "x");
        write_files(reversed.path(), &["charlie", "bravo", "alpha"], "x");

        assert_eq!(
            relative_entry_names(forward.path()),
            vec!["alpha", "bravo", "charlie"]
        );
        assert_eq!(
            relative_entry_names(reversed.path()),
            relative_entry_names(forward.path())
        );
    }

    // A staged subtree that cannot be read is not a shorter tree: the entries
    // behind it would be published unscanned and would be missing from the
    // archive pushed for the same digest, so two workers would hold different
    // content under one digest. The walk has to say so rather than drop them.
    #[test]
    fn staged_entries_reports_a_subtree_it_cannot_read() {
        let root = TempDir::new().unwrap();
        let staged = root.path().join("staged");
        let locked = staged.join("locked");

        std::fs::create_dir_all(&locked).unwrap();
        write_files(&staged, &["a.txt"], "a");
        std::fs::set_permissions(&locked, Permissions::from_mode(0o000)).unwrap();

        // Root traverses a mode-0 directory, so the injection is a no-op there
        // and everything below would be a false green.
        if std::fs::read_dir(&locked).is_ok() {
            std::fs::set_permissions(&locked, Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let walked = staged_entries(&staged);

        std::fs::set_permissions(&locked, Permissions::from_mode(0o755)).unwrap();

        let err = walked.expect_err("an unreadable subtree was walked as if it were empty");

        assert!(
            err.message().contains("failed to read staged output"),
            "{err:?}"
        );
    }

    // The pull producer's happy path, end to end through a real archive: the
    // files it carries land at the real output path and no staging directory
    // survives beside it.
    #[tokio::test]
    async fn publish_unpacked_publishes_the_files_a_real_archive_carries() {
        let (root, store_path, output_path) = store_dir();
        let archive_path = root.path().join("abc123.tar.zst");

        write_zstd_archive(
            &archive_path,
            &[("bin", None), ("bin/tool", Some("tool-bytes"))],
        )
        .await;

        publish_unpacked(&archive_path, &output_path).await.unwrap();

        assert_eq!(
            std::fs::read_to_string(output_path.join("bin").join("tool")).unwrap(),
            "tool-bytes"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "a pull left its staging directory under the store"
        );
    }

    // An archive of directories alone carries no bytes, and publishing it would
    // cache emptiness at that digest for every `exists()` reader forever. A pull
    // is the other way such an entry reaches a shared store path, so the seam's
    // rule has to bind it too.
    #[tokio::test]
    async fn publish_unpacked_refuses_an_archive_that_unpacks_to_no_files() {
        let (root, store_path, output_path) = store_dir();
        let archive_path = root.path().join("abc123.tar.zst");

        write_zstd_archive(&archive_path, &[("bin", None)]).await;

        let err = publish_unpacked(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(err.message().contains("no output files"), "{err:?}");
        assert!(
            !output_path.exists(),
            "a fileless archive was published onto the shared store path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "a refused pull left its staging directory under the store"
        );
    }

    // The other half of the content predicate: a symlink carries an artifact's
    // bytes by pointing at them, which is the whole output of a wrapper
    // artifact. Refusing it would convert a legitimate build into a failure.
    #[tokio::test]
    async fn a_producer_whose_only_output_is_a_symlink_publishes() {
        let (_root, store_path, output_path) = store_dir();

        let outcome = stage_then_publish(&output_path, |staging_path| async move {
            std::fs::create_dir_all(staging_path.join("bin")).unwrap();
            std::os::unix::fs::symlink("/elsewhere/tool", staging_path.join("bin").join("tool"))
                .unwrap();

            Ok::<(), Status>(())
        })
        .await
        .unwrap();

        assert_eq!(outcome, PublishOutcome::Published);
        assert_eq!(
            std::fs::read_link(output_path.join("bin").join("tool")).unwrap(),
            Path::new("/elsewhere/tool")
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()])
        );
    }

    // What a refused build has to give back. The lock is the one that matters:
    // digests are recipe-addressed, so a lock left behind refuses every later
    // build of that recipe until someone deletes the file by hand.
    #[tokio::test]
    async fn release_failed_build_removes_the_workspace_and_the_lock() {
        let root = TempDir::new().unwrap();
        let workspace_path = root.path().join("workspace");
        let lock_path = root.path().join("abc123.lock");

        std::fs::create_dir_all(workspace_path.join("source")).unwrap();
        write_files(&workspace_path, &["script.sh"], "echo hi");
        std::fs::write(&lock_path, "{}").unwrap();

        release_failed_build(&workspace_path, &lock_path).await;

        assert!(!workspace_path.exists(), "the workspace survived a failure");
        assert!(!lock_path.exists(), "the lock survived a failure");
    }

    // Cleanup runs while a failure is already being reported, so it must not
    // become a failure of its own — the caller has no error to replace and
    // nothing to report a second one through.
    #[tokio::test]
    async fn release_failed_build_tolerates_nothing_to_release() {
        let root = TempDir::new().unwrap();

        release_failed_build(&root.path().join("gone"), &root.path().join("gone.lock")).await;
    }

    // `step.environments` is a free-form list of request strings, so an entry
    // that names no variable arrives from the wire. Indexing past the end of a
    // split panicked the build task, which runs neither the seam's discard nor
    // the caller's lock release.
    #[test]
    fn expand_env_ignores_an_entry_that_names_no_variable() {
        let malformed = "no-separator".to_string();
        let output = "VORPAL_OUTPUT=/store/.tmp-uuid".to_string();

        assert_eq!(
            expand_env("$VORPAL_OUTPUT/bin", &[&malformed, &output]),
            "/store/.tmp-uuid/bin"
        );
    }

    // The value is everything after the first separator, so a flag-carrying
    // value survives substitution intact.
    #[test]
    fn expand_env_keeps_a_value_that_contains_a_separator() {
        let flags = "VORPAL_FLAGS=--define=x".to_string();

        assert_eq!(expand_env("$VORPAL_FLAGS", &[&flags]), "--define=x");
    }

    /// A build request carrying one dependency digest and one source, with
    /// every other field valid. `UnknownSystem` is the target because the
    /// target check is the first thing past the shape checks below: a request
    /// that reaches it has been accepted by all of them, and it is refused
    /// before the worker touches the filesystem or the network.
    fn build_request(
        namespace: &str,
        dependency_digest: &str,
        source_digest: &str,
    ) -> BuildArtifactRequest {
        BuildArtifactRequest {
            artifact: Some(Artifact {
                aliases: vec![],
                name: "artifact".to_string(),
                sources: vec![ArtifactSource {
                    digest: Some(source_digest.to_string()),
                    excludes: vec![],
                    includes: vec![],
                    name: "source".to_string(),
                    path: ".".to_string(),
                }],
                steps: vec![ArtifactStep {
                    arguments: vec![],
                    artifacts: vec![dependency_digest.to_string()],
                    entrypoint: Some("/bin/true".to_string()),
                    environments: vec![],
                    secrets: vec![],
                    script: None,
                }],
                systems: vec![],
                target: ArtifactSystem::UnknownSystem.into(),
            }),
            artifact_aliases: vec![],
            artifact_namespace: namespace.to_string(),
            registry: "http://localhost:0".to_string(),
        }
    }

    // Every existing negative-control test names a registry the worker
    // accepts, so its allow-list is fixed to exactly that one value: the
    // registry check passes and each test still exercises the check it was
    // written for.
    fn default_registry_allowed() -> Vec<String> {
        vec!["http://localhost:0".to_string()]
    }

    async fn build_refusal(request: BuildArtifactRequest) -> Status {
        build_refusal_with_registries(request, &default_registry_allowed()).await
    }

    async fn build_refusal_with_registries(
        request: BuildArtifactRequest,
        registry_allowed: &[String],
    ) -> Status {
        let (tx, _rx) = mpsc::channel(100);

        build_artifact(None, None, None, None, registry_allowed, request, &tx)
            .await
            .expect_err("a build request with an invalid field is refused")
    }

    fn valid_digest(fill: &str) -> String {
        fill.repeat(64 / fill.len())
    }

    // The namespace names a directory under every store root. A request that
    // supplies a path instead of a component is answered as a bad request,
    // before any path is composed from it.
    #[tokio::test]
    async fn build_artifact_refuses_a_namespace_that_is_not_one_path_component() {
        let digest = valid_digest("a");

        for hostile in ["../../../etc", "/etc", "", ".", "..", "a/b"] {
            let status = build_refusal(build_request(hostile, &digest, &digest)).await;

            assert_eq!(status.code(), tonic::Code::InvalidArgument, "{hostile:?}");
            assert!(
                status.message().contains("namespace"),
                "{hostile:?}: {}",
                status.message()
            );
        }

        // Positive control: the same request with a valid namespace and valid
        // digests is not refused here — it reaches the target check.
        let status = build_refusal(build_request("library", &digest, &digest)).await;

        assert_eq!(status.message(), "unknown target");
    }

    // A dependency digest is joined into the store path a dependency is pulled
    // to and run from, so it is refused unless it is a bare sha256 digest.
    #[tokio::test]
    async fn build_artifact_refuses_a_dependency_digest_that_is_not_a_bare_hex_string() {
        let digest = valid_digest("a");

        for hostile in ["../../../etc/passwd", "/etc/passwd", "", &digest[..63]] {
            let status = build_refusal(build_request("library", hostile, &digest)).await;

            assert_eq!(status.code(), tonic::Code::InvalidArgument, "{hostile:?}");
            assert!(
                status.message().contains("digest"),
                "{hostile:?}: {}",
                status.message()
            );
        }

        let status = build_refusal(build_request("library", &digest, &digest)).await;

        assert_eq!(status.message(), "unknown target");
    }

    // A source digest reaches `pull_source`, which joins it into the archive
    // path the source is downloaded to.
    #[tokio::test]
    async fn build_artifact_refuses_a_source_digest_that_is_not_a_bare_hex_string() {
        let digest = valid_digest("a");

        for hostile in ["../../../etc/passwd", "/etc/passwd", ""] {
            let status = build_refusal(build_request("library", &digest, hostile)).await;

            assert_eq!(status.code(), tonic::Code::InvalidArgument, "{hostile:?}");
            assert!(
                status.message().contains("digest"),
                "{hostile:?}: {}",
                status.message()
            );
        }

        let status = build_refusal(build_request("library", &digest, &digest)).await;

        assert_eq!(status.message(), "unknown target");
    }

    // `resolve_registry` is the seam the request-shape block calls. Unit tests
    // pin its cases directly; the `build_artifact` tests below confirm it is
    // actually wired in ahead of the target check.
    #[test]
    fn resolve_registry_defaults_to_the_sole_configured_value_when_the_request_is_silent() {
        let allowed = vec!["http://registry.example.com:9000".to_string()];

        assert_eq!(
            resolve_registry("", &allowed).unwrap(),
            "http://registry.example.com:9000"
        );
    }

    #[test]
    fn resolve_registry_refuses_a_mismatch_even_with_a_sole_configured_value() {
        let allowed = vec!["http://registry.example.com:9000".to_string()];

        let err = resolve_registry("http://attacker.example.com", &allowed).unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn resolve_registry_defaults_to_the_first_entry_when_the_request_is_silent() {
        let allowed = vec![
            "http://registry-a.example.com".to_string(),
            "http://registry-b.example.com".to_string(),
        ];

        assert_eq!(
            resolve_registry("", &allowed).unwrap(),
            "http://registry-a.example.com"
        );
    }

    #[test]
    fn resolve_registry_accepts_a_listed_selection() {
        let allowed = vec![
            "http://registry-a.example.com".to_string(),
            "http://registry-b.example.com".to_string(),
        ];

        assert_eq!(
            resolve_registry("http://registry-b.example.com", &allowed).unwrap(),
            "http://registry-b.example.com"
        );
    }

    #[test]
    fn resolve_registry_trims_one_trailing_slash_on_both_sides() {
        let allowed = vec!["http://registry.example.com/".to_string()];

        assert_eq!(
            resolve_registry("http://registry.example.com", &allowed).unwrap(),
            "http://registry.example.com/"
        );
    }

    // The prior test only trims the allow-list side; a request-supplied
    // trailing slash must be trimmed too, or the two sides of the same
    // normalization would be tested asymmetrically.
    #[test]
    fn resolve_registry_trims_one_trailing_slash_on_the_request_side_too() {
        let allowed = vec!["http://registry.example.com".to_string()];

        assert_eq!(
            resolve_registry("http://registry.example.com/", &allowed).unwrap(),
            "http://registry.example.com"
        );
    }

    #[test]
    fn resolve_registry_refuses_a_selection_outside_the_allow_list() {
        let allowed = vec![
            "http://registry-a.example.com".to_string(),
            "http://registry-b.example.com".to_string(),
        ];

        let err = resolve_registry("http://attacker.example.com", &allowed).unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("registry"), "{}", err.message());
    }

    // Prefix and substring near-misses must not be admitted by a whole-URI
    // allow-list entry: `starts_with` would let a `.evil.test` suffix through,
    // and `contains` would let a query-string trick through.
    #[test]
    fn resolve_registry_refuses_prefix_and_substring_near_misses() {
        let allowed = vec!["https://registry.example.com".to_string()];

        for hostile in [
            "https://registry.example.com.evil.test",
            "https://evil.test/?u=https://registry.example.com",
        ] {
            let err = resolve_registry(hostile, &allowed).unwrap_err();

            assert_eq!(err.code(), tonic::Code::InvalidArgument, "{hostile:?}");
        }
    }

    #[test]
    fn resolve_registry_refuses_everything_when_nothing_is_configured() {
        let err = resolve_registry("http://registry.example.com", &[]).unwrap_err();

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    // A request naming a registry outside the allow-list is refused ahead of
    // the target check — the same "unknown target" positive control the other
    // request-shape tests use proves the ordering: it only appears once the
    // registry has passed.
    #[tokio::test]
    async fn build_artifact_refuses_a_registry_outside_the_allow_list() {
        let digest = valid_digest("a");
        let mut request = build_request("library", &digest, &digest);
        request.registry = "http://attacker.example.com".to_string();

        let status =
            build_refusal_with_registries(request, &["http://registry.example.com".to_string()])
                .await;

        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(
            status.message().contains("registry"),
            "{}",
            status.message()
        );
        assert_ne!(status.message(), "unknown target");
    }

    // Positive control: the allow-listed registry reaches the target check,
    // proving the refusal above is about the registry and not a side effect.
    #[tokio::test]
    async fn build_artifact_accepts_an_allow_listed_registry() {
        let digest = valid_digest("a");
        let mut request = build_request("library", &digest, &digest);
        request.registry = "http://registry.example.com".to_string();

        let status =
            build_refusal_with_registries(request, &["http://registry.example.com".to_string()])
                .await;

        assert_eq!(status.message(), "unknown target");
    }

    // `pull_artifact` composes store paths from its own arguments, so it makes
    // the same refusal rather than trusting the caller to have made it.
    #[tokio::test]
    async fn pull_artifact_refuses_a_digest_that_is_not_a_bare_hex_string() {
        let (tx, _rx) = mpsc::channel(100);
        let registry = resolve_registry("", &["http://localhost:0".to_string()]).unwrap();

        let status = pull_artifact(None, "library", "../../../etc", &registry, &tx)
            .await
            .expect_err("a traversing digest is refused");

        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("digest"), "{}", status.message());
    }

    // C4/C5 (threat model): the registry refusal must run before
    // `obtain_service_credentials`, which performs its own OIDC discovery
    // round trip. Drives `build_artifact` directly with real issuer
    // credentials pointed at a listener that accepts the TCP connection but
    // never answers — if the refusal ran after the credential fetch, the
    // discovery request would hang against it and the outer `timeout` below
    // would elapse instead of the call returning promptly.
    #[tokio::test]
    async fn build_artifact_registry_refusal_precedes_obtaining_service_credentials() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local listener");
        let issuer_addr = listener.local_addr().expect("local addr");

        tokio::spawn(async move {
            // Accept and hold each connection open with no response, so any
            // OIDC discovery request against this issuer would hang rather
            // than fail fast.
            while let Ok((stream, _)) = listener.accept().await {
                std::mem::forget(stream);
            }
        });

        let digest = valid_digest("a");
        let mut request = build_request("library", &digest, &digest);
        request.registry = "http://attacker.example.com".to_string();

        let issuer = format!("http://{issuer_addr}");
        let (tx, _rx) = mpsc::channel(100);

        // The exact budget is not the discriminator — a refusal that runs
        // ahead of `obtain_service_credentials` returns in microseconds,
        // while one that reaches the hung listener would block far longer
        // than any of this test's real work. 5s gives that gap comfortable
        // headroom under CI load without buying any real signal from a
        // tighter number (C10 reconcile: the original 500ms budget was a
        // flake source with no discriminating value of its own).
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            build_artifact(
                Some(&issuer),
                None,
                Some("client-id"),
                Some("client-secret"),
                &["http://registry.example.com".to_string()],
                request,
                &tx,
            ),
        )
        .await;

        let status = result
            .expect(
                "registry refusal must return before obtain_service_credentials \
                 contacts the issuer",
            )
            .expect_err("a build request naming an unlisted registry is refused");

        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(
            status.message().contains("registry"),
            "{}",
            status.message()
        );
    }
}
