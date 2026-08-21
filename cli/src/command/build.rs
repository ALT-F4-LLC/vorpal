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
    collections::HashMap,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::exit,
};
use tokio::{
    fs::{create_dir_all, read_link, rename, symlink_metadata, write, File},
    io::{AsyncReadExt, BufReader},
};
use tonic::{transport::Channel, Code, Request};
use tracing::{error, info};
use vorpal_sdk::{
    api::{
        agent::agent_service_client::AgentServiceClient,
        archive::{archive_service_client::ArchiveServiceClient, ArchivePullRequest},
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
fn parse_artifact_namespace(namespace: &str) -> Result<()> {
    if namespace.is_empty()
        || namespace == "."
        || namespace == ".."
        || namespace.contains('/')
        || namespace.contains('\\')
    {
        bail!(
            "invalid artifact namespace {:?}: must be non-empty, contain no path separator, \
             and not be '.' or '..'",
            namespace,
        );
    }

    Ok(())
}

/// The prefix every staging path is named with (`staging_path_for`,
/// `store/paths.rs`). This producer's corpus is archive content from
/// whoever the registry forwards, not content this process staged itself, so
/// an archive that embeds this prefix hands whoever crafted it a path a
/// concurrent build's own staging traffic will pass through once this
/// producer's rename retires the directory the archive named. Unlike the
/// worker's own embedded-reference scan (which matches the one staging name
/// a single build used), this check matches the general prefix.
const STAGING_PATH_NEEDLE: &[u8] = b".tmp-";

const STAGING_SCAN_CHUNK_SIZE: usize = 8192;

/// Scans every regular file and symlink target under `staged_files` for
/// `STAGING_PATH_NEEDLE`, in overlapping chunks so a match straddling a read
/// boundary is still found. Returns the first offending path, if any.
async fn find_staging_path_reference(staged_files: &[PathBuf]) -> Result<Option<PathBuf>> {
    let needle = STAGING_PATH_NEEDLE;
    let overlap = needle.len() - 1;

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

            if buf[..filled].windows(needle.len()).any(|w| w == needle) {
                return Ok(Some(path.clone()));
            }

            carried = overlap.min(filled);
            buf.copy_within(filled - carried..filled, 0);
        }
    }

    Ok(None)
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
    let mut stream_data = Vec::new();

    loop {
        match stream.message().await {
            Ok(Some(chunk)) => {
                if !chunk.data.is_empty() {
                    stream_data.extend_from_slice(&chunk.data);
                }
            }

            Ok(None) => break,

            Err(status) => {
                if status.code() != Code::NotFound {
                    bail!("registry stream error ({error_label}): {status:?}");
                }

                break;
            }
        }
    }

    if !stream_data.is_empty() {
        publish_archive_bytes(&stream_data, archive_path).await?;
    }

    Ok(())
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

/// Writes `data` to a staged sibling of `archive_path`, then publishes it
/// with a single rename onto the shared store path. No reader of
/// `archive_path` ever observes a partial or truncated file — mirrors the
/// worker's own publish path (`cli/src/command/start/worker.rs`).
async fn publish_archive_bytes(data: &[u8], archive_path: &Path) -> Result<()> {
    let archive_parent = archive_path
        .parent()
        .ok_or_else(|| anyhow!("failed to get archive parent path"))?;

    create_dir_all(archive_parent).await?;

    let staging_path = staging_path_for(archive_path);

    let staged: Result<()> = async {
        write(&staging_path, data)
            .await
            .map_err(|err| anyhow!("failed to write archive {}: {err}", archive_path.display()))?;

        set_timestamps(&staging_path).await?;

        publish_atomically(&staging_path, archive_path)
            .await
            .map(|_| ())
    }
    .await;

    if staged.is_err() {
        discard_staging(&staging_path).await;
    }

    staged
}

/// Unpacks `archive_path` into a staged sibling of `output_path`, then
/// publishes it with a single rename, so the real output path is only ever
/// created — whole — by that rename. Mirrors the worker's own publish path.
async fn publish_unpacked_output(archive_path: &Path, output_path: &Path) -> Result<()> {
    let staging_path = staging_path_for(output_path);

    create_dir_all(&staging_path).await?;

    let staged: Result<()> = async {
        unpack_zstd(&staging_path, archive_path).await?;

        let staged_files = get_file_paths(&staging_path, vec![], vec![])?;

        // `get_file_paths` walks from the staging root down and reports every
        // entry, so its result is never empty for a directory that exists and
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

        if let Some(offender) = find_staging_path_reference(&staged_files).await? {
            // As with the emptiness bail above, the archive is the unusable
            // input: retire it so the next invocation re-pulls instead of
            // short-circuiting on a cached archive already rejected.
            retire_atomically(archive_path).await?;

            let offender = offender
                .strip_prefix(&staging_path)
                .unwrap_or(&offender)
                .display()
                .to_string();

            bail!(
                "archive embeds a staging-path reference in {offender}, which does not survive \
                 publishing"
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
                error!("{} |> {}", &artifact.name, err.message());
                exit(1);
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

async fn build_artifacts(
    artifact_namespace: &str,
    artifact_selected: Option<&Artifact>,
    artifact_selected_aliases: Vec<String>,
    build_store: &HashMap<String, Artifact>,
    client_archive: &mut ArchiveServiceClient<Channel>,
    client_worker: &mut WorkerServiceClient<Channel>,
    registry: &str,
) -> Result<()> {
    let artifact_order = get_order(build_store).await?;

    let mut build_complete = std::collections::HashSet::<String>::new();

    for artifact_digest in artifact_order {
        match build_store.get(&artifact_digest) {
            None => bail!("artifact 'config' not found: {artifact_digest}"),

            Some(artifact) => {
                for step in &artifact.steps {
                    for hash in &step.artifacts {
                        if !build_complete.contains(hash) {
                            bail!("artifact 'build' not found: {hash}");
                        }
                    }
                }

                let mut artifact_aliases = vec![];

                if let Some(selected) = artifact_selected {
                    if selected.name == artifact.name {
                        // loop can revisit this branch across iterations; can't move out of it once
                        artifact_aliases = artifact_selected_aliases.clone();
                    }
                }

                build(
                    artifact,
                    artifact_aliases,
                    &artifact_digest,
                    artifact_namespace,
                    client_archive,
                    client_worker,
                    registry,
                )
                .await?;

                build_complete.insert(artifact_digest);

                // Sources are managed by the agent, no artifact entries needed
            }
        }
    }

    Ok(())
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
    service: &RunArgsService,
    config_file: &Path,
) -> Result<HashMap<String, Artifact>> {
    let (mut config_process, mut config_client) = match start(
        &service.agent,
        &artifact.context,
        &artifact.name,
        &artifact.namespace,
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
            // `artifact` is a shared reference and `namespace` is reused in the loop below
            namespace: artifact.namespace.clone(),
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
            namespace: artifact.namespace.clone(),
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
    parse_artifact_namespace(&artifact.namespace)?;

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
        artifact.namespace.clone(),
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
    let mut client_archive = ArchiveServiceClient::new(client_archive_channel);

    let client_worker_channel = build_channel(&service.worker).await?;
    let mut client_worker = WorkerServiceClient::new(client_worker_channel);

    // Build config dependencies first to ensure config binary exists
    let config_store = config_context.get_artifact_store();

    build_artifacts(
        &artifact.namespace,
        None,
        vec![],
        config_store,
        &mut client_archive,
        &mut client_worker,
        &service.registry,
    )
    .await?;

    // Start configuration

    let config_file = resolve_config_file(&config_digest, &artifact.namespace, &config);
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

    let config_artifacts_store = collect_config_artifacts(&artifact, &service, config_file).await?;

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
            &artifact.namespace,
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
        output_path = get_artifact_output_path(&selected_artifact_digest, &artifact.namespace)
            .display()
            .to_string();
        &output_path
    } else {
        &selected_artifact_digest
    };

    build_artifacts(
        &artifact.namespace,
        Some(selected_artifact),
        artifact.aliases,
        &build_store,
        &mut client_archive,
        &mut client_worker,
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

    // Builds a real `.tar.zst` at `archive_path` holding `files` as regular
    // entries and `dirs` as directory entries. `compress_zstd` cannot serve
    // here: it stages through the real store root.
    async fn write_tar_zst(archive_path: &Path, files: &[(&str, &str)], dirs: &[&str]) {
        use tokio::io::AsyncWriteExt;

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

        for (name, contents) in files {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Regular);
            header.set_mode(0o644);
            header.set_size(contents.len() as u64);

            builder
                .append_data(&mut header, name, contents.as_bytes())
                .await
                .unwrap();
        }

        builder.finish().await.unwrap();

        let mut encoder = builder.into_inner().await.unwrap();

        encoder.shutdown().await.unwrap();
    }

    // AC3, archive half, pinned at the CLI's own call site: a pulled
    // archive is staged elsewhere and published onto its shared path, so a
    // reader gating on `archive_path.exists()` never opens a half-written
    // file, and a file already there is replaced whole rather than
    // truncated in place under a reader's open handle.
    #[tokio::test]
    async fn publish_archive_bytes_replaces_the_real_path_instead_of_writing_into_it() {
        let root = TempDir::new().unwrap();
        let archive_dir = root.path().join("archives");
        let archive_path = archive_dir.join("abc123.tar.zst");

        std::fs::create_dir_all(&archive_dir).unwrap();
        std::fs::write(&archive_path, b"first-bytes").unwrap();

        publish_archive_bytes(b"second-bytes", &archive_path)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&archive_path).unwrap(), b"second-bytes");
        assert_eq!(
            dir_entry_names(&archive_dir),
            BTreeSet::from(["abc123.tar.zst".to_string()]),
            "a staging file was left under the store directory"
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
    fn parse_artifact_namespace_refuses_traversal_and_separators() {
        for hostile in ["..", ".", "", "../escape", "a/b", "a\\b"] {
            let err = parse_artifact_namespace(hostile).unwrap_err();

            assert!(
                err.to_string().contains("invalid artifact namespace"),
                "accepted a namespace that escapes the store root: {hostile:?}"
            );
        }
    }

    #[test]
    fn parse_artifact_namespace_accepts_an_ordinary_namespace() {
        parse_artifact_namespace("library").unwrap();
        parse_artifact_namespace("my-namespace.v2").unwrap();
    }

    // C5, positive control: an archive whose regular files hold ordinary
    // content must still publish - the scan below must not turn into a
    // content check that rejects anything unexpected.
    #[tokio::test]
    async fn publish_unpacked_output_publishes_an_archive_with_ordinary_content() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(&archive_path, &[("bin", "ordinary binary content")], &[]).await;

        let output_path = store_path.join("abc123");

        publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap();

        assert_eq!(
            dir_entry_names(&output_path),
            BTreeSet::from(["bin".to_string()]),
        );
    }

    // C5 / AC-8: a registry-supplied archive whose only regular file embeds
    // a `.tmp-` staging-path reference must be refused rather than
    // published, so a local user cannot pre-create the directory a
    // concurrent build's staging traffic will pass through.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_an_archive_embedding_a_staging_path_reference() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(
            &archive_path,
            &[(
                "bin",
                "some content referencing /var/lib/vorpal/store/artifact/output/library/.tmp-deadbeef/x",
            )],
            &[],
        )
        .await;

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
            !archive_path.exists(),
            "the refused archive stayed cached, so every later build skips the pull and fails again"
        );
    }
}
