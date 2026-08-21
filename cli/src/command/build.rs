use crate::command::{
    config::{get_artifacts, get_order, start},
    lock::{artifact_system_to_platform, load_lock},
    store::{
        archives::unpack_zstd,
        paths::{
            discard_staging, get_artifact_archive_path, get_artifact_output_lock_path,
            get_artifact_output_path, get_file_paths, publish_atomically, set_timestamps,
            staging_path_for,
        },
    },
    VorpalConfigSource,
};
use anyhow::{anyhow, bail, Context, Result};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet},
    future::Future,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::exit,
};
use tokio::{
    fs::{create_dir_all, read_link, rename, symlink_metadata, File},
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    task::JoinSet,
};
use tonic::{transport::Channel, Code, Request, Streaming};
use tracing::{error, info};
use vorpal_sdk::{
    api::{
        agent::agent_service_client::AgentServiceClient,
        archive::{
            archive_service_client::ArchiveServiceClient, ArchivePullRequest, ArchivePullResponse,
        },
        artifact::{
            artifact_service_client::ArtifactServiceClient, Artifact, ArtifactRequest,
            ArtifactSystem, ArtifactsRequest,
        },
        worker::{worker_service_client::WorkerServiceClient, BuildArtifactRequest},
    },
    artifact::{
        language::{go::Go, python::Python, rust::Rust, typescript::TypeScript},
        protoc::Protoc,
        protoc_gen_go::ProtocGenGo,
        protoc_gen_go_grpc::ProtocGenGoGrpc,
        system::get_system_default_str,
    },
    context::{build_channel, client_auth_header, ConfigContext},
};
use walkdir::WalkDir;

/// Length of a sha256 digest in lowercase hex, the only shape a store path
/// component ever takes (`sdk/rust/src/context.rs` hashes artifact JSON with
/// `sha256::digest`).
const ARTIFACT_DIGEST_LENGTH: usize = 64;

/// A digest is joined straight into a store path by `get_artifact_output_path`
/// / `get_artifact_archive_path`, and the directory it names is later
/// executed from. Anything other than a bare sha256 hex string lets whoever
/// supplied it - the config channel, in this file - choose a destination
/// outside the store, so the shape is checked at every point a digest enters
/// this process. Mirrors `run.rs`'s `parse_artifact_digest`.
fn parse_artifact_digest(digest: &str, source: &str) -> Result<String> {
    let is_lowercase_hex = digest
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));

    if digest.len() != ARTIFACT_DIGEST_LENGTH || !is_lowercase_hex {
        bail!(
            "invalid artifact digest from {source}: expected {ARTIFACT_DIGEST_LENGTH} lowercase \
             hex characters, got {:?}",
            digest,
        );
    }

    Ok(digest.to_string())
}

/// A namespace is joined straight into a store path by
/// `get_artifact_output_dir_path` / `get_artifact_archive_dir_path`. A value
/// containing a path separator, or equal to `.` or `..`, escapes the store
/// root the same way a hostile digest would - and `Vorpal.toml`'s `namespace`
/// reaches here unvalidated (project config takes precedence over defaults),
/// so it is checked at the one place it enters this process.
///
/// Returns the component rather than `()` so the checked value is what
/// callers go on to use: a call site that drops the parse stops compiling
/// instead of quietly joining the unchecked string into a store path.
/// `run.rs` defines the same function, with the same predicate, for the
/// components it takes from an alias.
fn parse_store_path_component(value: &str, field: &str) -> Result<String> {
    let staging_prefix = staging_path_prefix();

    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.starts_with(&staging_prefix)
    {
        bail!(
            "invalid artifact {field} {:?}: must be non-empty, contain no path separator, \
             not be '.' or '..', and not start with the reserved staging prefix {:?}",
            value,
            staging_prefix,
        );
    }

    Ok(value.to_string())
}

/// Text length of the hyphenated UUID `staging_path_for` appends to every
/// staging path's name.
const STAGING_UUID_LENGTH: usize = 36;

const STAGING_SCAN_CHUNK_SIZE: usize = 8192;

/// The prefix `staging_path_for` (`store/paths.rs`) names every staging path
/// with, read back out of that function rather than written out here, so
/// renaming the prefix there cannot leave this file compiling and matching
/// nothing.
fn staging_path_prefix() -> String {
    let sample = staging_path_for(Path::new("/entry"));

    let name = sample
        .file_name()
        .expect("a staging path is a named sibling of the entry it stages")
        .to_string_lossy()
        .to_string();

    name[..name.len() - STAGING_UUID_LENGTH].to_string()
}

/// Whether `window` opens with a whole staging path component: a `/`
/// separator, the staging prefix, and a hyphenated-UUID-shaped name.
///
/// The shape is the anchor. A bare occurrence of the prefix is ordinary
/// content — a version string like `sequal.tmp-1.0.8`, a regular-expression
/// source, the format string inside vorpal's own binary — and matching it
/// would refuse legitimate artifacts, while what this control is looking for
/// is a reference to a store path some other producer stages through.
fn is_staging_path_reference(window: &[u8], needle: &[u8]) -> bool {
    if !window.starts_with(needle) {
        return false;
    }

    let name = &window[needle.len()..];

    name.len() >= STAGING_UUID_LENGTH
        && name[..STAGING_UUID_LENGTH]
            .iter()
            .enumerate()
            .all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => *byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            })
}

/// Reports the first staged entry whose contents or symlink target name a
/// staging path — `<store path>/.tmp-<uuid>`.
///
/// This producer's corpus is archive content from whoever the registry
/// forwards, not content this process staged itself, so an archive that
/// embeds such a path hands whoever crafted it a path a concurrent build's
/// own staging traffic will pass through once this producer's rename retires
/// the directory the archive named. Unlike the worker's own
/// embedded-reference scan (which matches the one staging name a single build
/// used), this one matches any staging name.
///
/// Files are read in overlapping chunks so a reference straddling a read
/// boundary is still found and an artifact larger than memory is still
/// scannable.
///
/// This finds a literal occurrence, so like the worker's own scan it binds a
/// naive producer only: an archive that encodes, compresses or splits the
/// path defeats it. It is a guard against an artifact recording a staging
/// path — accidentally, or to squat one — not a control that makes a hostile
/// archive safe to publish.
async fn find_staging_path_reference(staged_files: &[PathBuf]) -> Result<Option<PathBuf>> {
    let needle = format!("/{}", staging_path_prefix()).into_bytes();
    let window_size = needle.len() + STAGING_UUID_LENGTH;
    let overlap = window_size - 1;

    for path in staged_files.iter() {
        let metadata = symlink_metadata(path)
            .await
            .map_err(|err| anyhow!("failed to stat staged file {}: {err}", path.display()))?;

        if metadata.is_symlink() {
            let target = read_link(path)
                .await
                .map_err(|err| anyhow!("failed to read staged link {}: {err}", path.display()))?;

            if target
                .as_os_str()
                .as_bytes()
                .windows(window_size)
                .any(|window| is_staging_path_reference(window, &needle))
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
                .map_err(|err| anyhow!("failed to open staged file {}: {err}", path.display()))?,
        );

        let mut buf = vec![0u8; STAGING_SCAN_CHUNK_SIZE + overlap];
        let mut carried = 0usize;

        loop {
            let read = reader
                .read(&mut buf[carried..])
                .await
                .map_err(|err| anyhow!("failed to read staged file {}: {err}", path.display()))?;

            if read == 0 {
                break;
            }

            let filled = carried + read;

            if buf[..filled]
                .windows(window_size)
                .any(|window| is_staging_path_reference(window, &needle))
            {
                return Ok(Some(path.clone()));
            }

            carried = overlap.min(filled);
            buf.copy_within(filled - carried..filled, 0);
        }
    }

    Ok(None)
}

/// Every entry under a staged tree, with no exclusions and no entry dropped:
/// this is the corpus the scan above judges, so a subtree missing from it is
/// a subtree that publishes unread. `get_file_paths` cannot serve — it drops
/// `.git` unconditionally and swallows any entry the walk cannot read.
fn staged_entry_paths(staging_path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for entry in WalkDir::new(staging_path) {
        let entry = entry.map_err(|err| {
            anyhow!(
                "failed to walk staged tree {}: {err}",
                staging_path.display()
            )
        })?;

        paths.push(entry.path().to_path_buf());
    }

    paths.sort();

    Ok(paths)
}

/// Artifact-level `build`/`prepare` arguments, mirroring the independent
/// boolean CLI flags on `Command::Build` one-to-one.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors independent CLI flags on Command::Build one-to-one; a state machine or enum would not fit clap's per-flag parsing"
)]
pub struct RunArgsArtifact {
    pub aliases: Vec<String>,
    pub context: PathBuf,
    pub export: bool,
    pub list: bool,
    pub name: String,
    pub namespace: String,
    pub jobs: usize,
    pub path: bool,
    pub prepare_only: bool,
    pub rebuild: bool,
    pub system: String,
    pub unlock: bool,
    pub variable: Vec<String>,
}

/// Role A (config-binary compile target, fed to `ConfigContext::new`) vs Role B
/// (artifact target, fed unchanged to `start()`'s `--artifact-system` arg) are
/// conflated under one `--system` value for `build`. `prepare` must always
/// compile+run the config binary host-natively (it executes locally to
/// enumerate the graph), while the artifact graph and pinned sources still
/// target whatever `--system` was requested - so Role A is host-native only
/// when `prepare_only`, and Role B is untouched everywhere.
fn resolve_config_system(prepare_only: bool, requested_system: &str) -> String {
    if prepare_only {
        get_system_default_str()
    } else {
        requested_system.to_string()
    }
}

/// Classifies a source's pin relative to its prior `Vorpal.lock` state:
/// no prior entry -> first-use trust decision (`mint`); prior entry with a
/// different digest -> trust rotation (`update`); prior entry with the same
/// digest -> unchanged, merely re-verified (`verify`). This is the audit
/// trail that justifies defaulting `prepare`'s `--unlock` to `true`.
fn classify_pin(prior_digest: Option<&str>, current_digest: &str) -> &'static str {
    match prior_digest {
        None => "mint",
        Some(digest) if digest != current_digest => "update",
        Some(_) => "verify",
    }
}

pub struct RunArgsConfig {
    pub context: PathBuf,
    pub environments: Vec<String>,
    pub language: String,
    pub name: String,
    pub source: Option<VorpalConfigSource>,
}

pub struct RunArgsService {
    pub agent: String,
    pub registry: String,
    pub worker: String,
}

/// Unpacks `archive_path` into a staged sibling of `artifact_path` and
/// publishes it with a single rename, if the archive is present. Returns
/// whether output files exist afterward.
async fn unpack_archive_if_present(
    artifact_name: &str,
    artifact_digest: &str,
    artifact_path: &Path,
    archive_path: &Path,
) -> Result<bool> {
    if !archive_path.exists() {
        return Ok(false);
    }

    info!("{artifact_name} |> unpack: {artifact_digest}");

    publish_unpacked_output(archive_path, artifact_path).await?;

    let artifact_files = get_file_paths(&artifact_path.to_path_buf(), vec![], vec![])?;

    Ok(!artifact_files.is_empty())
}

/// An archive being written into a staged sibling of its real store path.
/// Bytes land in the staged file as they arrive and the real path is only
/// ever created — whole — by `publish`'s single rename, so no reader of it
/// observes a partial or truncated archive (mirrors the worker's own publish
/// path, `cli/src/command/start/worker.rs`).
///
/// Writing incrementally is what keeps peak memory to one chunk per
/// concurrent pull: accumulating a whole archive into a `Vec<u8>` first made
/// peak CLI memory `jobs` times the largest concurrent archive, and an
/// archive's size is the registry's choice rather than the user's, so
/// `--jobs`'s own ceiling was bounding the wrong unit.
struct StagedArchive {
    archive_path: PathBuf,
    bytes_written: usize,
    file: File,
    staging_path: PathBuf,
}

impl StagedArchive {
    async fn create(archive_path: &Path) -> Result<Self> {
        let archive_parent = archive_path
            .parent()
            .ok_or_else(|| anyhow!("failed to get archive parent path"))?;

        create_dir_all(archive_parent).await?;

        let staging_path = staging_path_for(archive_path);
        let file = File::create(&staging_path)
            .await
            .map_err(|err| anyhow!("failed to write archive {}: {err}", archive_path.display()))?;

        Ok(Self {
            archive_path: archive_path.to_path_buf(),
            bytes_written: 0,
            file,
            staging_path,
        })
    }

    async fn write_chunk(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data).await.map_err(|err| {
            anyhow!(
                "failed to write archive {}: {err}",
                self.archive_path.display()
            )
        })?;

        self.bytes_written += data.len();

        Ok(())
    }

    /// Publishes the staged bytes onto the real archive path, reporting
    /// whether anything was published: an archive that carried no bytes at
    /// all publishes nothing and leaves the store path absent, so the caller
    /// falls through to building it. The staged file is discarded on every
    /// path that does not publish, this one included.
    async fn publish(mut self) -> Result<bool> {
        let published: Result<bool> = async {
            self.file.flush().await?;

            if self.bytes_written == 0 {
                return Ok(false);
            }

            set_timestamps(&self.staging_path).await?;

            publish_atomically(&self.staging_path, &self.archive_path).await?;

            Ok(true)
        }
        .await;

        // The compensator runs on the `Err` and empty arms: a *dropped*
        // future is neither, and leaves the staged file behind. That a drop
        // never happens on the build path is `run_scheduler`'s
        // drain-never-abort property (see its C-3 paragraph), not a property
        // of this type — adding a cancellation point above this call (a
        // `select!`, a timeout, a ctrl-C handler) without a drop guard here
        // orphans staged archives, which nothing reaps.
        if !matches!(published, Ok(true)) {
            discard_staging(&self.staging_path).await;
        }

        published
    }

    async fn discard(self) {
        discard_staging(&self.staging_path).await;
    }
}

/// Streams `stream`'s chunks straight into a `StagedArchive` and publishes
/// it, reporting whether anything was published.
///
/// A `NotFound` stream error ends the pull with whatever arrived, matching
/// the pull request's own "the registry does not have it" path; any other
/// stream error is an error and the staged archive is discarded.
async fn publish_archive_stream(
    stream: &mut Streaming<ArchivePullResponse>,
    archive_path: &Path,
    error_context: &str,
) -> Result<bool> {
    let mut staged = StagedArchive::create(archive_path).await?;

    loop {
        match stream.message().await {
            Ok(Some(chunk)) => {
                if !chunk.data.is_empty() {
                    if let Err(err) = staged.write_chunk(&chunk.data).await {
                        staged.discard().await;

                        return Err(err);
                    }
                }
            }

            Ok(None) => break,

            Err(status) => {
                if status.code() != Code::NotFound {
                    staged.discard().await;

                    bail!("{error_context}: {:?}", status);
                }

                break;
            }
        }
    }

    staged.publish().await
}

/// Pulls `artifact_digest`'s archive from the registry into `archive_path` if
/// it is not already present locally. A registry `NotFound` is not an error:
/// it means the archive genuinely has no output (e.g. a source-only
/// artifact), so the archive is simply left absent. `on_not_found` logs the
/// caller's context-specific message for that case.
async fn pull_archive(
    client_archive: &mut ArchiveServiceClient<Channel>,
    archive_path: &PathBuf,
    artifact_digest: &str,
    artifact_namespace: &str,
    registry: &str,
    error_label: &str,
    on_not_found: impl FnOnce(),
) -> Result<()> {
    if archive_path.exists() {
        return Ok(());
    }

    let request = ArchivePullRequest {
        digest: artifact_digest.to_string(),
        namespace: artifact_namespace.to_string(),
    };

    let mut request = Request::new(request);
    let request_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {e}"))?;

    if let Some(header) = request_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    let response = match client_archive.pull(request).await {
        Err(status) => {
            if status.code() != Code::NotFound {
                bail!("{error_label}: {status:?}");
            }

            on_not_found();

            return Ok(());
        }

        Ok(response) => response,
    };

    let mut stream = response.into_inner();

    publish_archive_stream(&mut stream, archive_path, error_label).await?;

    Ok(())
}

/// Unpacks `archive_path` into a staged sibling of `output_path`, then
/// publishes it with a single rename, so the real output path is only ever
/// created — whole — by that rename. Mirrors the worker's own publish path.
async fn publish_unpacked_output(archive_path: &Path, output_path: &Path) -> Result<()> {
    let staging_path = staging_path_for(output_path);

    create_dir_all(&staging_path).await?;

    let staged: Result<()> = async {
        unpack_zstd(&staging_path, archive_path).await?;

        let staged_files = staged_entry_paths(&staging_path)?;

        // `staged_entry_paths` walks from the staging root down and reports
        // every entry, so its result is never empty for a directory that exists and
        // counting it says nothing about content: an archive of no entries, or
        // of directory entries only, unpacks to a tree with no regular file in
        // it. Publishing that leaves a store path every later `exists()` check
        // reads as a finished artifact and no reader can use, so refuse before
        // the rename. This is a smoke test for "the unpack produced something",
        // never evidence the tree is the intended one — the archive bytes are
        // not verified against the digest anywhere.
        if !staged_files.iter().any(|path| path.is_file()) {
            // The archive is the unusable input, so retire it too: otherwise
            // the `archive_path.exists()` check short-circuits the pull on
            // every later invocation and this digest can never be built again,
            // not even with `--rebuild`.
            retire_atomically(archive_path).await?;

            bail!("archive unpacked no files: {}", archive_path.display());
        }

        let scanned = match find_staging_path_reference(&staged_files).await {
            Ok(scanned) => scanned,

            // The scan failed on this host - an unreadable staged file, a
            // full disk - rather than judging the archive. Retire the archive
            // so the next invocation re-pulls and scans it, instead of
            // short-circuiting forever on a cached archive that was never
            // judged.
            Err(err) => {
                retire_atomically(archive_path).await?;

                return Err(err);
            }
        };

        if let Some(offender) = scanned {
            // The archive is kept, unlike the emptiness bail above: this
            // verdict is a function of the archive's bytes alone, so a
            // re-pull of the same digest reproduces it exactly. Retiring it
            // would turn a permanent refusal into a download on every
            // invocation. The next one refuses again, from cache, with the
            // same message.
            let offender = offender
                .strip_prefix(&staging_path)
                .unwrap_or(&offender)
                .display()
                .to_string()
                .escape_default()
                .to_string();

            bail!(
                "archive embeds a staging-path reference in \"{offender}\", which does not \
                 survive publishing"
            );
        }

        for path in staged_files.iter() {
            set_timestamps(path).await?;
        }

        publish_atomically(&staging_path, output_path)
            .await
            .map(|_| ())
    }
    .await;

    // As in `publish_archive_bytes`: `Err` is compensated, a dropped future
    // is not. This unpacked staging tree is the larger of the two — an
    // orphaned one holds a whole unpacked artifact — and nothing reaps
    // staging names, so this arm depends on `run_scheduler` draining rather
    // than aborting (its C-3 paragraph) for as long as it stays a plain
    // `is_err()` check rather than a drop guard.
    if staged.is_err() {
        discard_staging(&staging_path).await;
    }

    staged
}

/// Retires `real_path` (file or directory) so a concurrent reader observes
/// the whole entry or nothing, never a partial removal mid-`remove_dir_all`:
/// renames it onto a staging sibling, then discards the staged copy.
/// Retiring a path that is already gone is a no-op, not an error.
async fn retire_atomically(real_path: &Path) -> Result<()> {
    if !real_path.exists() {
        return Ok(());
    }

    let staging_path = staging_path_for(real_path);

    rename(real_path, &staging_path)
        .await
        .map_err(|err| anyhow!("failed to retire {}: {err}", real_path.display()))?;

    discard_staging(&staging_path).await;

    Ok(())
}

/// Builds one artifact: pull, unpack, or build-then-pull. This is the body
/// `run_scheduler` spawns, so it — and everything it calls — must report
/// failure by returning `Err` and never by `process::exit` or a panic
/// (C-4/AB-4). A task that kills the process takes every sibling build in
/// flight with it, skipping each one's staging compensator and its
/// credential-refresh commit window. The `exit` calls in `run()` are a
/// different case: they run on the single driver task before any dispatch.
async fn build(
    artifact: &Artifact,
    artifact_aliases: Vec<String>,
    artifact_digest: &str,
    artifact_namespace: &str,
    client_archive: &mut ArchiveServiceClient<Channel>,
    client_worker: &mut WorkerServiceClient<Channel>,
    registry: &str,
) -> Result<()> {
    // 1. Check artifact

    let artifact_path = get_artifact_output_path(artifact_digest, artifact_namespace);

    if artifact_path.exists() {
        return Ok(());
    }

    // 2. Pull

    let archive_path = get_artifact_archive_path(artifact_digest, artifact_namespace);

    pull_archive(
        client_archive,
        &archive_path,
        artifact_digest,
        artifact_namespace,
        registry,
        "registry pull error",
        || {},
    )
    .await?;

    if archive_path.exists() {
        let has_files = unpack_archive_if_present(
            &artifact.name,
            artifact_digest,
            &artifact_path,
            &archive_path,
        )
        .await?;

        if !has_files {
            bail!("Artifact files not found: {}", artifact_path.display());
        }

        return Ok(());
    }

    // Build

    let request = BuildArtifactRequest {
        // `artifact` is `&Artifact` and read again below (name logged in the stream loop)
        artifact: Some(artifact.clone()),
        artifact_aliases,
        artifact_namespace: artifact_namespace.to_string(),
        registry: registry.to_string(),
    };

    let mut request = Request::new(request);
    let request_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {e}"))?;

    if let Some(header) = request_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    let response = client_worker
        .build_artifact(request)
        .await
        .context("failed to build")?;

    let mut stream = response.into_inner();

    loop {
        match stream.message().await {
            Ok(Some(response)) => {
                if !response.output.is_empty() {
                    info!("{} |> {}", &artifact.name, response.output);
                }
            }

            Ok(None) => break,

            Err(err) => {
                // A spawned scheduler task cannot terminate the process:
                // that would kill every sibling build in flight with no
                // drain (C-4, AB-4). Return the error instead so the
                // scheduler can stop dispatching and drain what is already
                // running.
                bail!(
                    "{} |> worker stream error: {}",
                    &artifact.name,
                    err.message()
                );
            }
        };
    }

    // Pull built artifact back from registry to CLI host

    if !artifact_path.exists() {
        let archive_path = get_artifact_archive_path(artifact_digest, artifact_namespace);

        pull_archive(
            client_archive,
            &archive_path,
            artifact_digest,
            artifact_namespace,
            registry,
            "registry pull error after build",
            || {
                info!(
                    "{} |> artifact has no output files (not found in registry)",
                    artifact.name
                );
            },
        )
        .await?;

        unpack_archive_if_present(
            &artifact.name,
            artifact_digest,
            &artifact_path,
            &archive_path,
        )
        .await?;
    }

    Ok(())
}

/// How many stuck digests the scheduler's fail-closed `bail!` names before
/// summarising the rest as a count.
const STUCK_DIGESTS_LISTED: usize = 8;

/// Ready-set scheduler over `tokio::task::JoinSet`: dispatches every key of
/// `build_store` through `dispatch`, at most `jobs` concurrently, never
/// starting a node before every digest in its own `steps[].artifacts` has
/// completed. `order` is the dispatch order: among nodes that are ready at
/// the same moment, the one earliest in `order` goes first, whether it was
/// ready from the start or freed by a completion, so `--jobs 1` reproduces
/// `order` exactly (C-8) — pass `get_order`'s toposort output. A plain FIFO
/// queue does not: it runs every initially-ready root before any node a
/// completion freed, which is a different sequence from `order` on any graph
/// whose toposort interleaves a root's dependents before the next root.
///
/// C-1: the dependency invariant is checked against `build_store`'s own
/// edges before any dispatch, and an edge naming a digest `build_store` does
/// not have is a refusal, not a filter. `get_order`'s `DiGraphMap` invents a
/// node for a dangling edge (`config.rs:380-384`), so `order` is a superset
/// of a safe node set and is never used as one here — only `build_store`'s
/// keys are.
///
/// C-2: termination is explicit. The loop exits only when nothing is
/// in-flight; what happens next — return `Ok`, return the first error, or
/// `bail!` naming a stuck count — is decided from `completed` and
/// `first_error` afterward, never by the ready queue alone going empty.
///
/// C-3: no task is aborted on any error path. On the first `Err` (or a
/// spawned task's panic, surfaced as a `JoinError`), dispatch stops — but
/// every task already spawned is still drained to completion via
/// `join_next()`, because one of `build()`'s await points sits inside the
/// process-global credential refresh lock, and cancelling a task there can
/// lose a just-rotated refresh token (`sdk/rust/src/context.rs:1382-1385`).
/// The staging compensators in `StagedArchive::publish` and
/// `publish_unpacked_output` are `Err`-arm checks rather than drop guards,
/// and lean on this same property for their own coverage.
///
/// The guarantee is over this function's error paths, not over every way its
/// frame can be left: an unwind through the scheduler itself drops the
/// `JoinSet` with tasks live, which does abort them. Every panic reachable
/// here is a programmer-error `expect` over a map this function has just
/// built, so it is latent rather than reachable from any graph a caller can
/// supply — but a new fallible step in this loop must return, not panic.
///
/// C-7: dispatch is deduplicated by the bare digest string alone — the ready
/// queue, `indegree`, and `dependents` are all keyed on it, never on a tuple
/// carrying aliases or a parent, so a digest shared by two parents in a
/// diamond graph is still only ever spawned once.
async fn run_scheduler<F, Fut>(
    build_store: &HashMap<String, Artifact>,
    order: Vec<String>,
    jobs: usize,
    dispatch: F,
) -> Result<()>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    // The invariant lives with the function that depends on it: `jobs == 0`
    // would leave the dispatch guard below false forever and report a bad
    // argument as a stuck dependency graph. Callers clamp it too
    // (`clamp_jobs`, `cli/src/command.rs`), which is a convention a second
    // construction site does not inherit.
    if jobs == 0 {
        bail!("build scheduler needs at least one job, got 0");
    }

    for artifact in build_store.values() {
        for step in artifact.steps.iter() {
            for hash in step.artifacts.iter() {
                if !build_store.contains_key(hash) {
                    bail!("artifact 'build' not found: {}", hash);
                }
            }
        }
    }

    let mut indegree: HashMap<String, usize> = build_store
        .keys()
        .map(|digest| (digest.clone(), 0))
        .collect();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

    // A node's position in `order` is its dispatch priority among nodes ready
    // at the same moment (C-8).
    let position: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(index, digest)| (digest.as_str(), index))
        .collect();

    for digest in &order {
        let artifact = build_store
            .get(digest)
            .expect("order is a build_store key once the C-1 refusal above has passed");

        let mut seen: HashSet<&str> = HashSet::new();

        for step in artifact.steps.iter() {
            for dep in step.artifacts.iter() {
                if seen.insert(dep.as_str()) {
                    *indegree
                        .get_mut(digest)
                        .expect("digest is a build_store key") += 1;

                    dependents
                        .entry(dep.clone())
                        .or_default()
                        .push(digest.clone());
                }
            }
        }
    }

    // `Reverse` makes the max-heap pop the *smallest* position first, so the
    // node earliest in `order` is always the next one dispatched.
    let mut ready: BinaryHeap<Reverse<(usize, &str)>> = order
        .iter()
        .enumerate()
        .filter(|(_, digest)| indegree.get(digest.as_str()) == Some(&0))
        .map(|(index, digest)| Reverse((index, digest.as_str())))
        .collect();

    let node_count = build_store.len();
    // The completed set rather than a counter: which digests finished is what
    // the fail-closed `bail!` below has to name, and a count cannot say.
    let mut completed: HashSet<String> = HashSet::new();
    let mut in_flight = 0usize;
    let mut join_set: JoinSet<(String, Result<()>)> = JoinSet::new();
    let mut first_error: Option<anyhow::Error> = None;

    loop {
        while first_error.is_none() && in_flight < jobs {
            let Some(Reverse((_, digest))) = ready.pop() else {
                break;
            };

            let task_digest = digest.to_string();
            let fut = dispatch(digest.to_string());

            join_set.spawn(async move { (task_digest, fut.await) });

            in_flight += 1;
        }

        if in_flight == 0 {
            break;
        }

        let joined = join_set
            .join_next()
            .await
            .expect("in_flight tracks the live JoinSet length");

        in_flight -= 1;

        match joined {
            // A panicking task has no digest to report and no dependents to
            // free — only an error to carry out.
            Err(join_err) => {
                if first_error.is_none() {
                    first_error = Some(anyhow!("build task panicked: {join_err}"));
                }
            }

            Ok((digest, Ok(()))) => {
                // Nothing re-reads `ready` once `first_error` is set — the
                // dispatch loop above is gated on it — so freeing dependents
                // here after a failure is inert rather than a second path
                // out of the queue.
                if let Some(waiting) = dependents.get(&digest) {
                    for dependent in waiting {
                        let entry = indegree
                            .get_mut(dependent)
                            .expect("dependent is a build_store key");

                        *entry -= 1;

                        if *entry == 0 {
                            let index = *position
                                .get(dependent.as_str())
                                .expect("dependent is in order");

                            ready.push(Reverse((index, dependent.as_str())));
                        }
                    }
                }

                completed.insert(digest);
            }

            Ok((_, Err(err))) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    if let Some(err) = first_error {
        return Err(err);
    }

    if completed.len() != node_count {
        // Which artifacts are still waiting is what an operator needs here:
        // this bail fires on a graph the scheduler cannot complete, which a
        // hostile or broken config binary is what produces, and a bare count
        // leaves them nothing to look at. Capped, since such a graph can be
        // any size.
        let mut stuck: Vec<&str> = build_store
            .keys()
            .filter(|digest| !completed.contains(digest.as_str()))
            .map(|digest| digest.as_str())
            .collect();

        stuck.sort_unstable();

        let listed = stuck
            .iter()
            .take(STUCK_DIGESTS_LISTED)
            .copied()
            .collect::<Vec<&str>>()
            .join(", ");

        let remainder = stuck.len().saturating_sub(STUCK_DIGESTS_LISTED);

        bail!(
            "build scheduler made no further progress with {} of {} artifacts complete; \
             waiting on: {}{}",
            completed.len(),
            node_count,
            listed,
            if remainder > 0 {
                format!(" (and {remainder} more)")
            } else {
                String::new()
            },
        );
    }

    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "driver function threading namespace/selection/store/clients/jobs/registry through \
              the scheduler; grouping would only relocate the count, not reduce it"
)]
async fn build_artifacts(
    artifact_namespace: &str,
    artifact_selected: Option<&Artifact>,
    artifact_selected_aliases: Vec<String>,
    build_store: HashMap<String, Artifact>,
    client_archive: ArchiveServiceClient<Channel>,
    client_worker: WorkerServiceClient<Channel>,
    jobs: usize,
    registry: &str,
) -> Result<()> {
    // Still called once: `get_order`'s cycle detection stands (config.rs:387-390),
    // and its toposort output seeds the scheduler's deterministic FIFO tie-break.
    let artifact_order = get_order(&build_store).await?;

    let dispatch = |digest: String| {
        let artifact = build_store
            .get(&digest)
            .cloned()
            .expect("scheduler only dispatches digests present in build_store");

        let mut artifact_aliases = vec![];

        if let Some(selected) = artifact_selected {
            if selected.name == artifact.name {
                artifact_aliases = artifact_selected_aliases.clone();
            }
        }

        // Both clients are `#[derive(Clone)]` over a `Channel`, a cheap
        // multiplexed handle - cloning per dispatch is the mechanism, not
        // overhead.
        let mut client_archive = client_archive.clone();
        let mut client_worker = client_worker.clone();
        let artifact_namespace = artifact_namespace.to_string();
        let registry = registry.to_string();

        async move {
            build(
                &artifact,
                artifact_aliases,
                &digest,
                &artifact_namespace,
                &mut client_archive,
                &mut client_worker,
                &registry,
            )
            .await
        }
    };

    run_scheduler(&build_store, artifact_order, jobs, dispatch).await
}

/// Builds the config binary for `config.language` (go, rust, python, or
/// typescript) using `config_context`, returning its artifact digest.
#[expect(
    clippy::too_many_lines,
    reason = "four-way language dispatch (go/rust/python/typescript); splitting into four fns of identical shape only relabels the same arms"
)]
async fn build_config_binary(
    config: &RunArgsConfig,
    config_system: ArtifactSystem,
    config_context: &mut ConfigContext,
) -> Result<String> {
    match config.language.as_str() {
        "go" => {
            let protoc = Protoc::new().build(config_context).await?;
            let protoc_gen_go = ProtocGenGo::new().build(config_context).await?;
            let protoc_gen_go_grpc = ProtocGenGoGrpc::new().build(config_context).await?;

            let source_path = format!("{}.go", config.name);

            let mut includes = vec![&source_path, "go.mod", "go.sum"];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i
                    .iter()
                    .map(std::string::String::as_str)
                    .collect::<Vec<&str>>();
            }

            let mut builder = Go::new(&config.name, vec![config_system])
                .with_artifacts(vec![protoc, protoc_gen_go, protoc_gen_go_grpc])
                .with_includes(includes);

            if !config.environments.is_empty() {
                builder = builder.with_environments(
                    config
                        .environments
                        .iter()
                        .map(std::string::String::as_str)
                        .collect(),
                );
            }

            if let Some(script) = config.source.as_ref().and_then(|s| s.script.as_ref()) {
                builder = builder.with_source_script(script);
            }

            if let Some(directory) = config
                .source
                .as_ref()
                .and_then(|s| s.go.as_ref())
                .and_then(|g| g.directory.as_ref())
            {
                builder = builder.with_build_directory(directory);
            }

            builder.build(config_context).await
        }

        "rust" => {
            let mut bins = vec![config.name.as_str()];
            let bin_path = format!("src/{}.rs", config.name);
            let mut includes = vec![&bin_path, "Cargo.toml", "Cargo.lock"];
            let mut packages = vec![];

            if let Some(b) = config.source.as_ref().and_then(|s| s.rust.as_ref()) {
                if let Some(bin) = b.bin.as_ref() {
                    bins = vec![bin.as_str()];
                }

                if let Some(p) = b.packages.as_ref() {
                    packages = p
                        .iter()
                        .map(std::string::String::as_str)
                        .collect::<Vec<&str>>();
                }
            }

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i
                    .iter()
                    .map(std::string::String::as_str)
                    .collect::<Vec<&str>>();
            }

            let mut builder = Rust::new(&config.name, vec![config_system])
                .with_bins(bins)
                .with_includes(includes)
                .with_packages(packages);

            if !config.environments.is_empty() {
                builder = builder.with_environments(
                    config
                        .environments
                        .iter()
                        .map(std::string::String::as_str)
                        .collect(),
                );
            }

            builder.build(config_context).await
        }

        "python" => {
            let entrypoint = config
                .source
                .as_ref()
                .and_then(|s| s.python.as_ref())
                .and_then(|p| p.entrypoint.as_ref())
                .map_or_else(|| format!("src/{}.py", config.name), ToString::to_string);

            // Python projects are multi-file: include the package source tree, not just
            // the single entrypoint (the one deliberate divergence from the TypeScript arm).
            // README.md is required here (not in the other language arms) because hatchling
            // reads `[project].readme` at build time; omitting it fails `uv sync` for any
            // config relying on this default (DKT-30, following the DKT-28 workaround).
            let mut includes = vec!["pyproject.toml", "uv.lock", "src", "README.md"];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                if !i.is_empty() {
                    includes = i
                        .iter()
                        .map(std::string::String::as_str)
                        .collect::<Vec<&str>>();
                }
            }

            let mut builder = Python::new(&config.name, vec![config_system])
                .with_entrypoint(&entrypoint)
                .with_includes(includes);

            if !config.environments.is_empty() {
                builder = builder.with_environments(
                    config
                        .environments
                        .iter()
                        .map(std::string::String::as_str)
                        .collect(),
                );
            }

            let working_dir = config
                .source
                .as_ref()
                .and_then(|s| s.python.as_ref())
                .and_then(|p| p.directory.as_ref());

            if let Some(directory) = working_dir {
                builder = builder.with_working_dir(directory);
            }

            builder.build(config_context).await
        }

        "typescript" => {
            let entrypoint = config
                .source
                .as_ref()
                .and_then(|s| s.typescript.as_ref())
                .and_then(|t| t.entrypoint.as_ref())
                .map_or_else(|| format!("src/{}.ts", config.name), ToString::to_string);

            let mut includes = vec![
                "bun.lock",
                "bun.lockb",
                "package.json",
                "tsconfig.json",
                &entrypoint,
            ];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                if !i.is_empty() {
                    includes = i
                        .iter()
                        .map(std::string::String::as_str)
                        .collect::<Vec<&str>>();
                }
            }

            let mut builder = TypeScript::new(&config.name, vec![config_system])
                .with_entrypoint(&entrypoint)
                .with_includes(includes);

            if !config.environments.is_empty() {
                builder = builder.with_environments(
                    config
                        .environments
                        .iter()
                        .map(std::string::String::as_str)
                        .collect(),
                );
            }

            let working_dir = config
                .source
                .as_ref()
                .and_then(|s| s.typescript.as_ref())
                .and_then(|t| t.directory.as_ref());

            if let Some(directory) = working_dir {
                builder = builder.with_working_dir(directory);
            }

            builder.build(config_context).await
        }

        other => {
            bail!(
                "Unsupported language '{other}' in Vorpal.toml\n\n  \
                 Supported languages are: go, python, rust, typescript\n\n  \
                 To fix this, update the 'language' field in your Vorpal.toml:\n    \
                 language = \"typescript\"  # or \"python\", \"rust\", or \"go\""
            );
        }
    }
}

/// Starts the config binary, fetches the full artifact store it enumerates,
/// then kills the config process. Returns the store keyed by digest.
async fn collect_config_artifacts(
    artifact: &RunArgsArtifact,
    namespace: &str,
    service: &RunArgsService,
    config_file: &Path,
) -> Result<HashMap<String, Artifact>> {
    let (mut config_process, mut config_client) = match start(
        &service.agent,
        &artifact.context,
        &artifact.name,
        namespace,
        &artifact.system,
        artifact.unlock,
        &artifact.variable,
        config_file,
        &service.registry,
    )
    .await
    {
        Ok(res) => res,
        Err(error) => {
            error!("{}", error);
            exit(1);
        }
    };

    let config_artifacts_response = match config_client
        .get_artifacts(ArtifactsRequest {
            digests: vec![],
            namespace: namespace.to_string(),
        })
        .await
    {
        Ok(res) => res,
        Err(error) => {
            error!("failed to get config: {}", error);
            exit(1);
        }
    };

    let config_artifacts_response = config_artifacts_response.into_inner();
    let mut config_artifacts_store = HashMap::<String, Artifact>::new();

    for digest in config_artifacts_response.digests {
        let digest = parse_artifact_digest(&digest, "config artifacts")?;

        let request = ArtifactRequest {
            // `digest` is reused below as the store key after the request is sent
            digest: digest.clone(),
            namespace: namespace.to_string(),
        };

        let response = match config_client.get_artifact(request).await {
            Ok(res) => res,
            Err(error) => {
                error!("failed to get artifact: {}", error);
                exit(1);
            }
        };

        config_artifacts_store.insert(digest, response.into_inner());
    }

    config_process.kill().await?;

    Ok(config_artifacts_store)
}

/// Retires the config and selected artifacts' existing output/lock files so
/// `--rebuild` forces both to be rebuilt from scratch. Each path is retired
/// via `retire_atomically` (rename onto a staging sibling, then discard), so
/// a concurrent reader of any of these shared paths observes the whole entry
/// or nothing, never a partial removal.
async fn remove_outputs_for_rebuild(
    config_digest: &str,
    selected_artifact_digest: &str,
    namespace: &str,
) -> Result<()> {
    retire_atomically(&get_artifact_output_lock_path(config_digest, namespace)).await?;

    retire_atomically(&get_artifact_output_path(config_digest, namespace)).await?;

    retire_atomically(&get_artifact_output_lock_path(
        selected_artifact_digest,
        namespace,
    ))
    .await?;

    retire_atomically(&get_artifact_output_path(
        selected_artifact_digest,
        namespace,
    ))
    .await?;

    Ok(())
}

/// Builds and prints the `--unlock`/prepare-only pin summary: one
/// `mint`/`update`/`verify` line per remote source across the build store,
/// relative to `pre_lock_digests`' prior `Vorpal.lock` state.
fn print_prepare_summary(
    build_store: &HashMap<String, Artifact>,
    pre_lock_digests: &HashMap<(String, String), String>,
    selected_artifact_digest: &str,
) {
    let mut summary_lines: Vec<String> = build_store
        .values()
        .flat_map(|build_artifact| {
            let platform = artifact_system_to_platform(build_artifact.target);

            build_artifact
                .sources
                .iter()
                .filter(|source| {
                    source.path.starts_with("http://") || source.path.starts_with("https://")
                })
                .map(|source| {
                    // HashMap<(String, String), _> has no Borrow<(&str, &str)>; both values are also reused in the format! below
                    let key = (source.name.clone(), platform.clone());
                    let digest = source.digest.as_deref().unwrap_or_default();
                    let status =
                        classify_pin(pre_lock_digests.get(&key).map(String::as_str), digest);

                    format!("{status}: {} ({}) -> {}", source.name, platform, digest)
                })
                .collect::<Vec<_>>()
        })
        .collect();

    summary_lines.sort();
    summary_lines.dedup();

    for line in &summary_lines {
        crate::output::line(line);
    }

    crate::output::line(selected_artifact_digest);
}

/// Resolves the compiled config binary's path under the config artifact's
/// output directory, exiting the process with a language-specific hint if
/// the build completed but the binary is missing.
fn resolve_config_file(config_digest: &str, namespace: &str, config: &RunArgsConfig) -> PathBuf {
    let config_file = get_artifact_output_path(config_digest, namespace)
        .join("bin")
        .join(&config.name);

    if !config_file.exists() {
        let lang_hint = match config.language.as_str() {
            "typescript" => {
                "\n\n  For TypeScript configs, this means the bun build --compile step\n  \
                             may have failed silently, or the binary was not placed in the\n  \
                             expected output location.\n\n  \
                             Try rebuilding with --level debug to see the full build output."
            }
            "python" => {
                "\n\n  For Python configs, this means the app-mode launcher was not\n  \
                             written to the expected bin/ path during the build step.\n\n  \
                             Try rebuilding with --level debug to see the full build output."
            }
            _ => "",
        };
        error!(
            "Compiled config binary not found: {}{}\n",
            config_file.display(),
            lang_hint
        );
        exit(1);
    }

    config_file
}

#[expect(
    clippy::too_many_lines,
    reason = "build pipeline orchestrator: sets up clients, builds the config binary, then dispatches to list/export/prepare/build; each stage is one already-extracted helper call"
)]
pub async fn run(
    artifact: RunArgsArtifact,
    config: RunArgsConfig,
    service: RunArgsService,
) -> Result<()> {
    let artifact_namespace = parse_store_path_component(&artifact.namespace, "namespace")?;

    // Setup service clients

    let client_agent_channel = build_channel(&service.agent).await?;
    let client_artifact_channel = build_channel(&service.registry).await?;

    let client_agent = AgentServiceClient::new(client_agent_channel);
    let client_artifact = ArtifactServiceClient::new(client_artifact_channel);

    let mode = if artifact.unlock {
        "unlocked"
    } else {
        "locked"
    };

    info!("mode: {}", mode);

    // Prepare config context

    // ConfigContext::new (sdk/rust) takes ownership; config/artifact/service fields
    // are read again by later stages (build_config_binary, build_artifacts, etc.)
    let mut config_context = ConfigContext::new(
        config.name.clone(),
        config.context.clone(),
        artifact_namespace.clone(),
        resolve_config_system(artifact.prepare_only, &artifact.system),
        artifact.unlock,
        artifact.variable.clone(),
        client_agent,
        client_artifact,
        0,
        service.registry.clone(),
    )?;

    let config_system = config_context.get_system();

    let config_digest = build_config_binary(&config, config_system, &mut config_context).await?;

    if config_digest.is_empty() {
        bail!(
            "No config digest was produced for language '{}'\n\n  \
             The config build completed but did not return a valid artifact digest.\n  \
             This may indicate an internal error in the {} language builder.\n\n  \
             Try running with --level debug for more details.",
            config.language,
            config.language
        );
    }

    let config_digest = parse_artifact_digest(&config_digest, "config build")?;

    // Prepare lock path early for incremental artifact updates

    let client_archive_channel = build_channel(&service.registry).await?;
    let client_archive = ArchiveServiceClient::new(client_archive_channel);

    let client_worker_channel = build_channel(&service.worker).await?;
    let client_worker = WorkerServiceClient::new(client_worker_channel);

    // Build config dependencies first to ensure config binary exists
    let config_store = config_context.get_artifact_store();

    build_artifacts(
        &artifact_namespace,
        None,
        vec![],
        config_store.clone(),
        client_archive.clone(),
        client_worker.clone(),
        artifact.jobs,
        &service.registry,
    )
    .await?;

    // Start configuration

    let config_file = resolve_config_file(&config_digest, &artifact_namespace, &config);
    let config_file = config_file.as_path();

    // Snapshot Vorpal.lock before config evaluation runs (and pins/updates
    // sources via the agent's prepare_artifact RPC), so the prepare-only
    // summary below can distinguish newly-minted pins from re-verified ones.
    let lock_path = artifact.context.join("Vorpal.lock");

    let pre_lock_digests: HashMap<(String, String), String> = if artifact.prepare_only {
        load_lock(&lock_path)
            .await
            .unwrap_or(None)
            .map(|lock| {
                lock.sources
                    .into_iter()
                    .map(|s| ((s.name, s.platform), s.digest))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let config_artifacts_store =
        collect_config_artifacts(&artifact, &artifact_namespace, &service, config_file).await?;

    let (selected_artifact_digest, selected_artifact) = config_artifacts_store
        .iter()
        .find(|(_, val)| val.name == artifact.name)
        .ok_or_else(|| anyhow!("selected 'artifact' not found: {}", artifact.name))?;

    let selected_artifact_digest =
        parse_artifact_digest(&selected_artifact_digest, "selected artifact")?;

    if artifact.rebuild {
        remove_outputs_for_rebuild(
            &config_digest,
            &selected_artifact_digest,
            &artifact_namespace,
        )
        .await?;
    }

    let mut build_store = HashMap::<String, Artifact>::new();

    get_artifacts(
        selected_artifact,
        &selected_artifact_digest,
        &mut build_store,
        &config_artifacts_store,
    )
    .await?;

    if artifact.prepare_only {
        print_prepare_summary(&build_store, &pre_lock_digests, &selected_artifact_digest);

        return Ok(());
    }

    if artifact.list {
        let order = get_order(&build_store).await?;

        let max_name_len = order
            .iter()
            .filter_map(|d| build_store.get(d))
            .map(|a| a.name.len())
            .max()
            .unwrap_or(0);

        for digest in order {
            if let Some(a) = build_store.get(&digest) {
                crate::output::line(format!("{:<max_name_len$}  {digest}", a.name));
            }
        }

        return Ok(());
    }

    if artifact.export {
        let export = serde_json::to_string_pretty(selected_artifact)
            .context("failed to serialize artifact")?;

        crate::output::line(export);

        return Ok(());
    }

    let output_path;
    let output: &str = if artifact.path {
        output_path = get_artifact_output_path(&selected_artifact_digest, &artifact_namespace)
            .display()
            .to_string();
        &output_path
    } else {
        &selected_artifact_digest
    };

    build_artifacts(
        &artifact_namespace,
        Some(selected_artifact),
        artifact.aliases,
        build_store,
        client_archive,
        client_worker,
        artifact.jobs,
        &service.registry,
    )
    .await?;

    // TODO: explore running post scripts

    crate::output::line(output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
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

    // A staging path as an artifact would embed it: an absolute store path
    // whose last component is what `staging_path_for` names one.
    const EMBEDDED_STAGING_PATH: &str = concat!(
        "/var/lib/vorpal/store/artifact/output/library/",
        ".tmp-0190dead-beef-7000-8000-0123456789ab/bin/x"
    );

    // Builds a real `.tar.zst` at `archive_path` holding `files` as regular
    // entries (name, contents, mode), `dirs` as directory entries and `links`
    // as symlink entries (name, target). `compress_zstd` cannot serve here:
    // it stages through the real store root.
    async fn write_tar_zst_entries(
        archive_path: &Path,
        files: &[(&str, &str, u32)],
        dirs: &[&str],
        links: &[(&str, &str)],
    ) {
        let file = tokio::fs::File::create(archive_path).await.unwrap();
        let encoder = async_compression::tokio::write::ZstdEncoder::new(file);
        let mut builder = tokio_tar::Builder::new(encoder);

        for name in dirs {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Directory);
            header.set_mode(0o755);
            header.set_size(0);

            builder
                .append_data(&mut header, name, tokio::io::empty())
                .await
                .unwrap();
        }

        for (name, contents, mode) in files {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Regular);
            header.set_mode(*mode);
            header.set_size(contents.len() as u64);

            builder
                .append_data(&mut header, name, contents.as_bytes())
                .await
                .unwrap();
        }

        for (name, target) in links {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Symlink);
            header.set_mode(0o777);
            header.set_size(0);
            header.set_link_name(target).unwrap();

            builder
                .append_data(&mut header, name, tokio::io::empty())
                .await
                .unwrap();
        }

        builder.finish().await.unwrap();

        let mut encoder = builder.into_inner().await.unwrap();

        encoder.shutdown().await.unwrap();
    }

    async fn write_tar_zst(archive_path: &Path, files: &[(&str, &str)], dirs: &[&str]) {
        let files: Vec<(&str, &str, u32)> = files
            .iter()
            .map(|(name, contents)| (*name, *contents, 0o644))
            .collect();

        write_tar_zst_entries(archive_path, &files, dirs, &[]).await;
    }

    // AC3, archive half, pinned at the CLI's own call site: a pulled
    // archive is staged elsewhere and published onto its shared path, so a
    // reader gating on `archive_path.exists()` never opens a half-written
    // file, and a file already there is replaced whole rather than
    // truncated in place under a reader's open handle.
    #[tokio::test]
    async fn staged_archive_replaces_the_real_path_instead_of_writing_into_it() {
        let root = TempDir::new().unwrap();
        let archive_dir = root.path().join("archives");
        let archive_path = archive_dir.join("abc123.tar.zst");

        std::fs::create_dir_all(&archive_dir).unwrap();
        std::fs::write(&archive_path, b"first-bytes").unwrap();

        let mut staged = StagedArchive::create(&archive_path).await.unwrap();

        // Two chunks, because chunks are how the pull actually arrives: the
        // published file must be their concatenation, not the last one.
        staged.write_chunk(b"second-").await.unwrap();
        staged.write_chunk(b"bytes").await.unwrap();

        assert!(staged.publish().await.unwrap());

        assert_eq!(std::fs::read(&archive_path).unwrap(), b"second-bytes");
        assert_eq!(
            dir_entry_names(&archive_dir),
            BTreeSet::from(["abc123.tar.zst".to_string()]),
            "a staging file was left under the store directory"
        );
    }

    // C-6: an archive that carried no bytes publishes nothing, so the caller
    // still sees `archive_path.exists() == false` and falls through to
    // building it — and the staged file it opened does not survive as debris
    // in the store directory.
    #[tokio::test]
    async fn staged_archive_publishes_nothing_when_no_byte_ever_arrived() {
        let root = TempDir::new().unwrap();
        let archive_dir = root.path().join("archives");
        let archive_path = archive_dir.join("abc123.tar.zst");

        let staged = StagedArchive::create(&archive_path).await.unwrap();

        assert!(
            !staged.publish().await.unwrap(),
            "an empty archive reported itself as published"
        );
        assert!(!archive_path.exists(), "an empty archive was published");
        assert!(
            dir_entry_names(&archive_dir).is_empty(),
            "an empty archive left a staging file behind"
        );
    }

    // C-6: a discarded pull (a stream error mid-archive) leaves the store
    // directory exactly as it found it.
    #[tokio::test]
    async fn staged_archive_discard_leaves_no_file_behind() {
        let root = TempDir::new().unwrap();
        let archive_dir = root.path().join("archives");
        let archive_path = archive_dir.join("abc123.tar.zst");

        let mut staged = StagedArchive::create(&archive_path).await.unwrap();

        staged.write_chunk(b"partial").await.unwrap();
        staged.discard().await;

        assert!(!archive_path.exists(), "a discarded pull published anyway");
        assert!(
            dir_entry_names(&archive_dir).is_empty(),
            "a discarded pull left a staging file behind"
        );
    }

    // AC1 at the CLI's own call site: publish_unpacked_output must never
    // unpack into the real output path. A dependency already published
    // there survives a pull whose archive turns out to be garbage, byte for
    // byte — an in-place unpack (the pre-fix `create_dir_all` +
    // `unpack_zstd` directly onto the real path) would create into that
    // path and then delete it while cleaning up.
    #[tokio::test]
    async fn publish_unpacked_output_leaves_an_already_published_output_path_untouched() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");
        let output_path = store_path.join("abc123");

        std::fs::create_dir_all(&output_path).unwrap();
        write_files(&output_path, &["published.txt"], "published-content");

        let archive_path = root.path().join("abc123.tar.zst");

        std::fs::write(&archive_path, "not a zstd archive").unwrap();

        publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert_eq!(
            dir_entry_names(&output_path),
            BTreeSet::from(["published.txt".to_string()]),
            "a failed unpack disturbed an already published output path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "a failed unpack left its staging directory under the store"
        );
    }

    // AC2, kill/error case: an unpack that fails must leave nothing at the
    // real output path, so the `exists()` cache-hit check a later build
    // performs can never mistake wreckage for a finished artifact.
    #[tokio::test]
    async fn publish_unpacked_output_leaves_the_real_output_path_absent_on_failure() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        std::fs::write(&archive_path, "not a zstd archive").unwrap();

        let output_path = store_path.join("abc123");

        publish_unpacked_output(&archive_path, &output_path)
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

    // C8, fail closed before the rename: an archive that unpacks to no
    // regular file — no entries at all, or directory entries only — must
    // never be published, because the `exists()` cache-hit check every later
    // build performs would read that empty tree as a finished artifact. The
    // refused archive is retired with it, so the next build re-pulls instead
    // of short-circuiting on a cached archive it already rejected.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_an_archive_that_unpacked_no_files() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(&archive_path, &[], &["bin"]).await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("unpacked no files"),
            "a file-less archive was not refused: {err}"
        );
        assert!(
            !output_path.exists(),
            "a file-less archive was published as a finished artifact"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "refusing the archive left its staging directory under the store"
        );
        assert!(
            !archive_path.exists(),
            "the refused archive stayed cached, so every later build skips the pull and fails again"
        );
    }

    // C8, the other half: a publish that fails for any reason other than a
    // lost race must surface as an error rather than collapse to `Ok`, and
    // must leave no staging behind. A regular file at the output path makes
    // the publishing rename fail `ENOTDIR`, which is a genuine I/O failure.
    #[tokio::test]
    async fn publish_unpacked_output_reports_a_failed_publish_and_leaves_no_staging() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(&archive_path, &[("bin", "binary-content")], &[]).await;

        let output_path = store_path.join("abc123");

        std::fs::write(&output_path, b"not a directory").unwrap();

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("failed to publish"),
            "a genuine publish failure did not surface as an error: {err}"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "a failed publish left its staging directory under the store"
        );
    }

    // AC5: `--rebuild` must retire a store path rather than empty it in
    // place, so a concurrent reader walking `bin/` (or executing a binary
    // from it) never observes the directory disintegrating underneath it.
    // This pins that retiring replaces the real path with "gone" atomically
    // (no reader can observe a half-emptied directory) rather than via
    // `remove_dir_all` walking the real path entry by entry.
    #[tokio::test]
    async fn retire_atomically_removes_the_real_path_and_leaves_no_staging_behind() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");
        let real_path = store_path.join("abc123");

        std::fs::create_dir_all(&real_path).unwrap();
        write_files(&real_path, &["bin"], "binary-content");

        retire_atomically(&real_path).await.unwrap();

        assert!(!real_path.exists(), "retire left the real path behind");
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "retire left a staging directory under the store"
        );
    }

    // Retiring a path that never existed (e.g. a rebuild of an artifact
    // that was never built) must be a no-op, not an error — the original
    // check-then-act code guarded every removal with `.exists()` for the
    // same reason.
    #[tokio::test]
    async fn retire_atomically_is_a_no_op_for_an_absent_path() {
        let root = TempDir::new().unwrap();
        let missing_path = root.path().join("does-not-exist");

        retire_atomically(&missing_path).await.unwrap();
    }

    #[test]
    fn resolve_config_system_forces_host_native_for_prepare_only() {
        let host = get_system_default_str();

        assert_eq!(resolve_config_system(true, "x86_64-darwin"), host);
        assert_eq!(resolve_config_system(true, &host), host);
    }

    #[test]
    fn resolve_config_system_passes_through_requested_system_for_build() {
        assert_eq!(
            resolve_config_system(false, "x86_64-darwin"),
            "x86_64-darwin"
        );
    }

    // Role A across the full target matrix: for prepare, the config binary always
    // compiles host-native, so the requested target (incl. the CI Linux-host ->
    // darwin-target case) is ignored for the config-binary compile target.
    #[test]
    fn resolve_config_system_ignores_every_requested_target_for_prepare() {
        let host = get_system_default_str();

        for requested in [
            "aarch64-darwin",
            "x86_64-darwin",
            "aarch64-linux",
            "x86_64-linux",
        ] {
            assert_eq!(resolve_config_system(true, requested), host);
        }
    }

    // Role B is untouched: for build every requested target flows through
    // unchanged, so today's behavior is byte-identical across the matrix.
    #[test]
    fn resolve_config_system_preserves_every_requested_target_for_build() {
        for requested in [
            "aarch64-darwin",
            "x86_64-darwin",
            "aarch64-linux",
            "x86_64-linux",
        ] {
            assert_eq!(resolve_config_system(false, requested), requested);
        }
    }

    #[test]
    fn classify_pin_no_prior_entry_is_mint() {
        assert_eq!(classify_pin(None, "abc123"), "mint");
    }

    #[test]
    fn classify_pin_matching_prior_entry_is_verify() {
        assert_eq!(classify_pin(Some("abc123"), "abc123"), "verify");
    }

    #[test]
    fn classify_pin_changed_prior_entry_is_update() {
        assert_eq!(classify_pin(Some("abc123"), "def456"), "update");
    }

    // C1: a digest is joined straight into a store path, so anything other
    // than a bare sha256 hex string must be refused before it reaches
    // `get_artifact_output_path` / `get_artifact_archive_path`.
    #[test]
    fn parse_artifact_digest_refuses_a_digest_that_is_not_a_bare_hex_string() {
        for hostile in [
            "../../../../../../etc/passwd",
            "/etc/passwd",
            "ABC123",
            &"a".repeat(63),
            &"a".repeat(65),
        ] {
            let err = parse_artifact_digest(hostile, "test").unwrap_err();

            assert!(
                err.to_string().contains("invalid artifact digest"),
                "accepted a digest that does not name a store path: {hostile}"
            );
        }
    }

    #[test]
    fn parse_artifact_digest_accepts_a_sha256_digest() {
        let digest = "a".repeat(64);

        assert_eq!(parse_artifact_digest(&digest, "test").unwrap(), digest);
    }

    // C1: `Vorpal.toml`'s namespace reaches this file unvalidated. A value
    // containing a path separator, or equal to `.` or `..`, escapes the
    // store root the same way a hostile digest would.
    #[test]
    fn parse_store_path_component_refuses_traversal_and_separators() {
        for hostile in ["..", ".", "", "../escape", "a/b", "a\\b"] {
            let err = parse_store_path_component(hostile, "namespace").unwrap_err();

            assert!(
                err.to_string().contains("invalid artifact namespace"),
                "accepted a namespace that escapes the store root: {hostile:?}"
            );
        }
    }

    // A namespace that opens with the staging prefix names a directory
    // indistinguishable from a staging path, so a later sweeper of
    // `.tmp-*` would take a published namespace for abandoned staging.
    #[test]
    fn parse_store_path_component_refuses_the_reserved_staging_prefix() {
        let reserved = format!("{}0190deadbeef", staging_path_prefix());

        let err = parse_store_path_component(&reserved, "namespace").unwrap_err();

        assert!(
            err.to_string().contains("invalid artifact namespace"),
            "accepted a namespace that names a staging path: {err}"
        );
    }

    #[test]
    fn parse_store_path_component_accepts_an_ordinary_component() {
        assert_eq!(
            parse_store_path_component("library", "namespace").unwrap(),
            "library"
        );
        assert_eq!(
            parse_store_path_component("my-namespace.v2", "namespace").unwrap(),
            "my-namespace.v2"
        );
    }

    // The scan's needle is derived from `staging_path_for` rather than
    // spelled out, so this pins the two together: renaming the prefix there
    // fails here rather than leaving the scan matching nothing.
    #[test]
    fn staging_path_prefix_is_the_name_staging_path_for_produces() {
        let staging_path = staging_path_for(Path::new("/store/entry"));
        let name = staging_path.file_name().unwrap().to_string_lossy();

        assert!(
            name.starts_with(&staging_path_prefix()),
            "the scan's needle no longer matches what staging_path_for names: {name}"
        );
        assert_eq!(
            name.len(),
            staging_path_prefix().len() + STAGING_UUID_LENGTH,
            "the staging name's uuid is no longer the length the scan assumes: {name}"
        );
    }

    // C5, positive control: the scan matches a staging path, not the prefix
    // that names one. Every string here contains that prefix and none of
    // them names a store path a producer stages through — a version suffix,
    // a plain word, a regular-expression source, and the real store path an
    // artifact is expected to reference — so all of them must publish.
    // vorpal's own `bin/vorpal` carries the fourth of these (the format
    // string in `staging_path_for`), and a scan matching the bare prefix
    // refuses it.
    #[tokio::test]
    async fn publish_unpacked_output_publishes_content_that_merely_contains_the_staging_prefix() {
        for content in [
            "ordinary binary content",
            "sequal.tmp-1.0.8",
            "tmp-0190dead-beef-7000-8000-0123456789ab",
            ".tmpfile",
            "/^\\.tmp-/ and \".tmp-{}\"",
            "/var/lib/vorpal/store/artifact/output/library/abc123/bin/x",
        ] {
            let root = TempDir::new().unwrap();
            let store_path = root.path().join("output");

            std::fs::create_dir_all(&store_path).unwrap();

            let archive_path = root.path().join("abc123.tar.zst");

            write_tar_zst(&archive_path, &[("bin", content)], &[]).await;

            let output_path = store_path.join("abc123");

            publish_unpacked_output(&archive_path, &output_path)
                .await
                .unwrap_or_else(|err| panic!("refused ordinary content {content:?}: {err}"));

            assert_eq!(
                dir_entry_names(&output_path),
                BTreeSet::from(["bin".to_string()]),
            );
        }
    }

    // C5 / AC-8: a registry-supplied archive whose only regular file embeds
    // a staging path must be refused rather than published, so a local user
    // cannot pre-create the directory a concurrent build's staging traffic
    // will pass through.
    //
    // The archive is kept: the verdict is a function of its bytes, so a
    // re-pull of the same digest reproduces it exactly and retiring it would
    // buy a download per invocation and nothing else.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_an_archive_embedding_a_staging_path_reference() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");
        let content = format!("some content referencing {EMBEDDED_STAGING_PATH}");

        write_tar_zst(&archive_path, &[("bin", &content)], &[]).await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "an archive embedding a staging-path reference was not refused: {err}"
        );
        assert!(
            !output_path.exists(),
            "an archive embedding a staging-path reference was published"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "refusing the archive left its staging directory under the store"
        );
        assert!(
            archive_path.exists(),
            "a refusal the archive's own bytes determine retired it, so the next build re-pulls \
             the same bytes to reach the same verdict"
        );
    }

    // The scan reads a chunk plus one window of overlap at a time, and a
    // hostile archive only has to place its reference across that boundary.
    // This one starts two bytes before it, so it is found only by carrying
    // the tail of one read into the next.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_a_reference_that_straddles_a_scan_chunk() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");
        let window = 1 + staging_path_prefix().len() + STAGING_UUID_LENGTH;
        let read_size = STAGING_SCAN_CHUNK_SIZE + window - 1;
        let offset = EMBEDDED_STAGING_PATH
            .find(&format!("/{}", staging_path_prefix()))
            .unwrap();
        let content = format!(
            "{}{EMBEDDED_STAGING_PATH}",
            "f".repeat(read_size - 2 - offset)
        );

        write_tar_zst(&archive_path, &[("bin", &content)], &[]).await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "a reference split across two scan chunks was not found: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
    }

    // A symlink target is a path an artifact resolves at run time, so a link
    // into a staging path is the same squat as the reference in a file's
    // bytes. The regular file is there to carry the tree past the emptiness
    // bail.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_a_symlink_into_a_staging_path() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst_entries(
            &archive_path,
            &[("bin", "ordinary binary content", 0o644)],
            &[],
            &[("lib", EMBEDDED_STAGING_PATH)],
        )
        .await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "a symlink into a staging path was not refused: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
    }

    // The scan's corpus is the whole staged tree. A dotted directory is
    // ordinary artifact content and is published like any other, so leaving
    // it out of the walk hands an archive a free hiding place.
    #[tokio::test]
    async fn publish_unpacked_output_scans_dotted_directories_too() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");
        let content = format!("some content referencing {EMBEDDED_STAGING_PATH}");

        write_tar_zst(
            &archive_path,
            &[
                ("bin", "ordinary binary content"),
                (".git/config", &content),
            ],
            &[".git"],
        )
        .await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "a reference under a dotted directory escaped the scan: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
    }

    // A scan that cannot read a staged file has judged nothing, so the
    // archive must not stay cached: the failure is this host's, and a later
    // invocation that finds the archive already there would skip the pull
    // and never scan those bytes at all.
    #[tokio::test]
    async fn publish_unpacked_output_retires_the_archive_when_the_scan_cannot_read_a_file() {
        use std::os::unix::fs::PermissionsExt;

        let root = TempDir::new().unwrap();
        let probe_path = root.path().join("probe");

        std::fs::write(&probe_path, b"probe").unwrap();
        std::fs::set_permissions(&probe_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        if std::fs::File::open(&probe_path).is_ok() {
            // Running with privileges that ignore file modes, so an
            // unreadable staged file cannot be arranged here.
            return;
        }

        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst_entries(&archive_path, &[("bin", "content", 0o000)], &[], &[]).await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("failed to open staged file"),
            "an unreadable staged file did not surface as a scan failure: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
        assert!(
            !archive_path.exists(),
            "an archive the scan never judged stayed cached, so every later build skips the pull"
        );
    }

    // --- run_scheduler ---
    //
    // These tests exercise the ready-set scheduler directly, through fake
    // `dispatch` closures, rather than through `build_artifacts`/`build()` -
    // `build()`'s own corpus is real gRPC clients and the store filesystem,
    // neither of which this scheduling logic touches (fakes over an internal
    // seam, per fragments/tdd-discipline.md).

    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::sync::Mutex as AsyncMutex;
    use vorpal_sdk::api::artifact::ArtifactStep;

    fn artifact_depending_on(deps: &[&str]) -> Artifact {
        Artifact {
            steps: vec![ArtifactStep {
                artifacts: deps.iter().map(|d| d.to_string()).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    // AC2 / C-8: a non-trivial dependency chain (A depends on B depends on
    // C) dispatches in strict topological order under a `jobs` well above
    // the chain's own width, because readiness - not `jobs` - is what
    // sequences it.
    #[tokio::test]
    async fn run_scheduler_never_starts_a_node_before_its_dependencies_complete() {
        let mut build_store = HashMap::new();
        build_store.insert("c".to_string(), artifact_depending_on(&[]));
        build_store.insert("b".to_string(), artifact_depending_on(&["c"]));
        build_store.insert("a".to_string(), artifact_depending_on(&["b"]));

        let order = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let started: Arc<AsyncMutex<Vec<String>>> = Arc::new(AsyncMutex::new(Vec::new()));

        let dispatch = {
            let started = started.clone();

            move |digest: String| {
                let started = started.clone();

                async move {
                    started.lock().await.push(digest);
                    Ok(())
                }
            }
        };

        run_scheduler(&build_store, order, 3, dispatch)
            .await
            .expect("a completable chain must succeed");

        assert_eq!(
            *started.lock().await,
            vec!["c".to_string(), "b".to_string(), "a".to_string()],
            "a dependency chain dispatched out of order"
        );
    }

    // AC1: independent artifacts run concurrently, observable as overlap -
    // peak simultaneous in-flight count reaching more than 1. Run on the
    // multi-threaded runtime the binary itself uses (`rt-multi-thread`,
    // cli/Cargo.toml), so the overlap pinned here is real parallelism rather
    // than cooperative interleaving on one thread.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_scheduler_runs_independent_nodes_concurrently_up_to_jobs() {
        let mut build_store = HashMap::new();
        build_store.insert("x".to_string(), artifact_depending_on(&[]));
        build_store.insert("y".to_string(), artifact_depending_on(&[]));

        let order = vec!["x".to_string(), "y".to_string()];
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let dispatch = {
            let in_flight = in_flight.clone();
            let peak = peak.clone();

            move |_digest: String| {
                let in_flight = in_flight.clone();
                let peak = peak.clone();

                async move {
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);

                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

                    in_flight.fetch_sub(1, Ordering::SeqCst);

                    Ok(())
                }
            }
        };

        run_scheduler(&build_store, order, 2, dispatch)
            .await
            .expect("two independent nodes must succeed");

        assert!(
            peak.load(Ordering::SeqCst) >= 2,
            "independent artifacts never overlapped in flight"
        );
    }

    // C-6 / AC1's other half: `jobs` is an upper bound, not just a target.
    // More ready nodes than slots is the only shape that can catch an
    // unbounded fan-out - every fixture whose node count equals `jobs`
    // passes identically with the bound removed, and an unbounded scheduler
    // is what the `--jobs` ceiling exists to prevent.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_scheduler_never_exceeds_jobs_in_flight() {
        let names = ["n1", "n2", "n3", "n4", "n5"];
        let mut build_store = HashMap::new();

        for name in names {
            build_store.insert(name.to_string(), artifact_depending_on(&[]));
        }

        let order: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let dispatch = {
            let in_flight = in_flight.clone();
            let peak = peak.clone();

            move |_digest: String| {
                let in_flight = in_flight.clone();
                let peak = peak.clone();

                async move {
                    let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);

                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

                    in_flight.fetch_sub(1, Ordering::SeqCst);

                    Ok(())
                }
            }
        };

        run_scheduler(&build_store, order, 2, dispatch)
            .await
            .expect("five independent nodes must succeed");

        assert!(
            peak.load(Ordering::SeqCst) <= 2,
            "peak in-flight was {}, above the requested --jobs 2",
            peak.load(Ordering::SeqCst)
        );
    }

    // `jobs = 0` is a refusal that names the argument, not a build that
    // stalls and reports itself as a stuck dependency graph. The clamp at
    // the CLI call site is a convention a second construction site does not
    // inherit; this is the invariant living with the function.
    #[tokio::test]
    async fn run_scheduler_refuses_zero_jobs() {
        let mut build_store = HashMap::new();
        build_store.insert("only".to_string(), artifact_depending_on(&[]));

        let err = run_scheduler(
            &build_store,
            vec!["only".to_string()],
            0,
            |digest: String| async move {
                let _ = digest;
                Ok(())
            },
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string().contains("at least one job"),
            "jobs = 0 was not reported as a bad argument: {err}"
        );
    }

    // C-1: an edge naming a digest `build_store` does not have is a
    // refusal before any dispatch, not a filter. Positive control in the
    // same shape: the dependency present builds both nodes.
    #[tokio::test]
    async fn run_scheduler_refuses_a_dangling_dependency_edge_before_dispatching_anything() {
        let mut build_store = HashMap::new();
        build_store.insert("child".to_string(), artifact_depending_on(&["missing"]));

        let dispatched = Arc::new(AsyncMutex::new(Vec::<String>::new()));
        let dispatch = {
            let dispatched = dispatched.clone();

            move |digest: String| {
                let dispatched = dispatched.clone();

                async move {
                    dispatched.lock().await.push(digest);
                    Ok(())
                }
            }
        };

        let err = run_scheduler(&build_store, vec!["child".to_string()], 4, dispatch)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("not found"),
            "a dangling dependency edge was not refused: {err}"
        );
        assert!(
            dispatched.lock().await.is_empty(),
            "a dangling dependency edge let dispatch proceed anyway"
        );
    }

    #[tokio::test]
    async fn run_scheduler_positive_control_builds_both_nodes_when_the_dependency_is_present() {
        let mut build_store = HashMap::new();
        build_store.insert("dep".to_string(), artifact_depending_on(&[]));
        build_store.insert("child".to_string(), artifact_depending_on(&["dep"]));

        let dispatched = Arc::new(AsyncMutex::new(Vec::<String>::new()));
        let dispatch = {
            let dispatched = dispatched.clone();

            move |digest: String| {
                let dispatched = dispatched.clone();

                async move {
                    dispatched.lock().await.push(digest);
                    Ok(())
                }
            }
        };

        run_scheduler(
            &build_store,
            vec!["dep".to_string(), "child".to_string()],
            4,
            dispatch,
        )
        .await
        .expect("a satisfied dependency must build both nodes");

        let mut got = dispatched.lock().await.clone();
        got.sort();
        assert_eq!(got, vec!["child".to_string(), "dep".to_string()]);
    }

    // C-2: a graph the scheduler can never complete (a genuine cycle - not
    // catchable by the C-1 check above, since every edge here does name a
    // real `build_store` key) fails closed rather than hanging or reporting
    // `Ok` on ready-queue exhaustion. Wrapped in a timeout so a regression to
    // the hang shape fails the test instead of blocking CI.
    #[tokio::test]
    async fn run_scheduler_fails_closed_on_a_cycle_instead_of_hanging_or_succeeding() {
        let mut build_store = HashMap::new();
        build_store.insert("a".to_string(), artifact_depending_on(&["b"]));
        build_store.insert("b".to_string(), artifact_depending_on(&["a"]));

        let order = vec!["a".to_string(), "b".to_string()];
        let dispatch = |digest: String| async move {
            let _ = digest;
            Ok(())
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_scheduler(&build_store, order, 2, dispatch),
        )
        .await
        .expect("a cycle must fail fast, not hang");

        let err = result.expect_err("a scheduler cycle returned Ok instead of failing closed");

        // The stuck digests, not just a count: this bail fires when a
        // hostile or broken config binary hands over a graph the scheduler
        // cannot complete, which is exactly when the operator has least
        // context to work out which artifacts were waiting.
        let message = err.to_string();

        assert!(
            message.contains("waiting on: a, b"),
            "the stuck digests were not named: {message}"
        );
    }

    // C-3: on the first error, dispatch stops but nothing already spawned is
    // aborted - `slow` runs to completion and its marker is observed, and
    // `after` (only reachable through `slow`'s own dependents) is never
    // dispatched because dependents are not processed once an error has been
    // recorded.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_scheduler_drains_in_flight_work_and_starts_nothing_new_after_a_failure() {
        let mut build_store = HashMap::new();
        build_store.insert("fail".to_string(), artifact_depending_on(&[]));
        build_store.insert("slow".to_string(), artifact_depending_on(&[]));
        build_store.insert("after".to_string(), artifact_depending_on(&["slow"]));

        let order = vec!["fail".to_string(), "slow".to_string(), "after".to_string()];
        let dispatched = Arc::new(AsyncMutex::new(Vec::<String>::new()));
        let slow_completed = Arc::new(AtomicUsize::new(0));

        let dispatch = {
            let dispatched = dispatched.clone();
            let slow_completed = slow_completed.clone();

            move |digest: String| {
                let dispatched = dispatched.clone();
                let slow_completed = slow_completed.clone();

                async move {
                    dispatched.lock().await.push(digest.clone());

                    if digest == "fail" {
                        bail!("fail task refused on purpose");
                    }

                    if digest == "slow" {
                        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                        slow_completed.store(1, Ordering::SeqCst);
                    }

                    Ok(())
                }
            }
        };

        let err = run_scheduler(&build_store, order, 2, dispatch)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("fail task refused on purpose"),
            "the returned error was not the failing task's own error: {err}"
        );
        assert_eq!(
            slow_completed.load(Ordering::SeqCst),
            1,
            "the slow sibling was cancelled instead of drained to completion"
        );
        assert!(
            !dispatched.lock().await.contains(&"after".to_string()),
            "a node was dispatched after the failure was already recorded"
        );
    }

    // C-3/C-4's other arm: a task that *panics* surfaces as a `JoinError`,
    // not a process exit, and is drained around exactly like a returned
    // `Err` - the sibling still runs to completion and the panic is reported
    // as this build's error. The returned-`Err` arm above never reaches this
    // code, so without this test the whole panicking-task path is unrun.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_scheduler_reports_a_panicking_task_and_still_drains_its_siblings() {
        let mut build_store = HashMap::new();
        build_store.insert("boom".to_string(), artifact_depending_on(&[]));
        build_store.insert("slow".to_string(), artifact_depending_on(&[]));

        let order = vec!["boom".to_string(), "slow".to_string()];
        let slow_completed = Arc::new(AtomicUsize::new(0));

        let dispatch = {
            let slow_completed = slow_completed.clone();

            move |digest: String| {
                let slow_completed = slow_completed.clone();

                async move {
                    if digest == "boom" {
                        panic!("boom task panicked on purpose");
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;

                    slow_completed.store(1, Ordering::SeqCst);

                    Ok(())
                }
            }
        };

        let err = run_scheduler(&build_store, order, 2, dispatch)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("build task panicked"),
            "a panicking task was not reported as a build failure: {err}"
        );
        assert_eq!(
            slow_completed.load(Ordering::SeqCst),
            1,
            "a sibling was cancelled instead of drained when a task panicked"
        );
    }

    // C-7: the join of a diamond graph - the node two completing parents can
    // each free - is dispatched exactly once, deduplicated by the bare digest
    // string alone. The root cannot double-dispatch (it is seeded once and
    // freed by nobody), so `d`, not `a`, is where the defect class lives.
    // Positive control in the same test: all four nodes are still dispatched.
    #[tokio::test]
    async fn run_scheduler_dispatches_a_shared_dependency_exactly_once() {
        let mut build_store = HashMap::new();
        build_store.insert("a".to_string(), artifact_depending_on(&[]));
        build_store.insert("b".to_string(), artifact_depending_on(&["a"]));
        build_store.insert("c".to_string(), artifact_depending_on(&["a"]));
        build_store.insert("d".to_string(), artifact_depending_on(&["b", "c"]));

        let order = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let dispatched = Arc::new(AsyncMutex::new(Vec::<String>::new()));

        let dispatch = {
            let dispatched = dispatched.clone();

            move |digest: String| {
                let dispatched = dispatched.clone();

                async move {
                    dispatched.lock().await.push(digest);
                    Ok(())
                }
            }
        };

        run_scheduler(&build_store, order, 4, dispatch)
            .await
            .expect("a diamond graph must complete");

        let dispatched = dispatched.lock().await;

        assert_eq!(
            dispatched.iter().filter(|d| *d == "d").count(),
            1,
            "the node two parents each freed was dispatched more than once"
        );

        let mut got = dispatched.clone();
        got.sort();
        assert_eq!(
            got,
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]
        );
    }

    // C-8: `--jobs 1` reproduces the seed order's dispatch sequence exactly.
    //
    // The graph is the one that tells order-indexed readiness apart from a
    // FIFO queue: `q` depends on `p`, `r` does not, and the order puts `q`
    // (the dependent) before `r` (the second root) - the interleaved shape
    // `get_order`'s reverse-postorder DFS actually produces. A FIFO ready
    // queue dispatches p, r, q, because both roots are seeded before `q` is
    // freed; only position-in-`order` gives p, q, r.
    #[tokio::test]
    async fn run_scheduler_jobs_one_reproduces_the_seed_order_exactly() {
        let mut build_store = HashMap::new();
        build_store.insert("p".to_string(), artifact_depending_on(&[]));
        build_store.insert("q".to_string(), artifact_depending_on(&["p"]));
        build_store.insert("r".to_string(), artifact_depending_on(&[]));

        let order = vec!["p".to_string(), "q".to_string(), "r".to_string()];
        let dispatched = Arc::new(AsyncMutex::new(Vec::<String>::new()));

        let dispatch = {
            let dispatched = dispatched.clone();

            move |digest: String| {
                let dispatched = dispatched.clone();

                async move {
                    dispatched.lock().await.push(digest);
                    Ok(())
                }
            }
        };

        run_scheduler(&build_store, order.clone(), 1, dispatch)
            .await
            .expect("a completable graph under jobs=1 must succeed");

        assert_eq!(
            *dispatched.lock().await,
            order,
            "--jobs 1 did not reproduce the toposort-seeded dispatch order"
        );
    }

    // A dependency named twice - here once per step, the shape a config
    // binary produces when two steps of one artifact both consume the same
    // toolchain - counts once. Indegree and dependents currently gain the
    // duplicate symmetrically, so the graph completes either way; this
    // fixture is what makes a later change to one side of that symmetry
    // (dependents as a set, say) fail instead of hanging.
    #[tokio::test]
    async fn run_scheduler_counts_a_repeated_dependency_digest_once() {
        let child = Artifact {
            steps: vec![
                ArtifactStep {
                    artifacts: vec!["dep".to_string()],
                    ..Default::default()
                },
                ArtifactStep {
                    artifacts: vec!["dep".to_string()],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let mut build_store = HashMap::new();
        build_store.insert("dep".to_string(), artifact_depending_on(&[]));
        build_store.insert("child".to_string(), child);

        let order = vec!["dep".to_string(), "child".to_string()];
        let dispatched = Arc::new(AsyncMutex::new(Vec::<String>::new()));

        let dispatch = {
            let dispatched = dispatched.clone();

            move |digest: String| {
                let dispatched = dispatched.clone();

                async move {
                    dispatched.lock().await.push(digest);
                    Ok(())
                }
            }
        };

        run_scheduler(&build_store, order.clone(), 2, dispatch)
            .await
            .expect("a repeated dependency edge must not stall the graph");

        assert_eq!(
            *dispatched.lock().await,
            order,
            "a dependency named twice was not counted once"
        );
    }
}
