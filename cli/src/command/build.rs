use crate::command::{
    config::{get_artifacts, get_order, start},
    lock::{artifact_system_to_platform, load_lock},
    store::{
        archives::unpack_zstd,
        paths::{
            get_artifact_archive_path, get_artifact_output_lock_path, get_artifact_output_path,
            get_file_paths, set_timestamps,
        },
    },
    VorpalConfigSource,
};
use anyhow::{anyhow, bail, Context, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::exit,
};
use tokio::fs::{create_dir_all, remove_dir_all, remove_file, write};
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
        let archive_path_parent = archive_path
            .parent()
            .ok_or_else(|| anyhow!("failed to get archive parent path"))?;

        create_dir_all(archive_path_parent).await?;

        write(archive_path, &stream_data)
            .await
            .context("failed to write archive")?;

        set_timestamps(archive_path).await?;
    }

    Ok(())
}

/// Unpacks `archive_path` into `artifact_path` and refreshes file timestamps,
/// if the archive is present. Returns whether output files exist afterward.
async fn unpack_archive_if_present(
    artifact_name: &str,
    artifact_digest: &str,
    artifact_path: &PathBuf,
    archive_path: &Path,
) -> Result<bool> {
    if !archive_path.exists() {
        return Ok(false);
    }

    info!("{artifact_name} |> unpack: {artifact_digest}");

    create_dir_all(artifact_path)
        .await
        .context("failed to create artifact path")?;

    unpack_zstd(artifact_path, archive_path).await?;

    let artifact_files = get_file_paths(artifact_path, vec![], vec![])?;

    for artifact_file in &artifact_files {
        set_timestamps(artifact_file).await?;
    }

    Ok(!artifact_files.is_empty())
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
    build_store: HashMap<String, Artifact>,
    client_archive: &mut ArchiveServiceClient<Channel>,
    client_worker: &mut WorkerServiceClient<Channel>,
    registry: &str,
) -> Result<()> {
    let artifact_order = get_order(&build_store).await?;

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

/// Removes the config and selected artifacts' existing output/lock files so
/// `--rebuild` forces both to be rebuilt from scratch.
async fn remove_outputs_for_rebuild(
    config_digest: &str,
    selected_artifact_digest: &str,
    namespace: &str,
) -> Result<()> {
    let config_artifact_output_lock_path = get_artifact_output_lock_path(config_digest, namespace);

    if config_artifact_output_lock_path.exists() {
        remove_file(&config_artifact_output_lock_path)
            .await
            .context("failed to remove config artifact lock file")?;
    }

    let config_artifact_output_path = get_artifact_output_path(config_digest, namespace);

    if config_artifact_output_path.exists() {
        remove_dir_all(&config_artifact_output_path)
            .await
            .context("failed to remove config artifact path")?;
    }

    let artifact_output_lock_path =
        get_artifact_output_lock_path(selected_artifact_digest, namespace);

    if artifact_output_lock_path.exists() {
        remove_file(&artifact_output_lock_path)
            .await
            .context("failed to remove artifact lock file")?;
    }

    let artifact_output_path = get_artifact_output_path(selected_artifact_digest, namespace);

    if artifact_output_path.exists() {
        remove_dir_all(&artifact_output_path)
            .await
            .context("failed to remove artifact path")?;
    }

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

    if artifact.rebuild {
        remove_outputs_for_rebuild(
            &config_digest,
            selected_artifact_digest,
            &artifact.namespace,
        )
        .await?;
    }

    let mut build_store = HashMap::<String, Artifact>::new();

    get_artifacts(
        selected_artifact,
        selected_artifact_digest,
        &mut build_store,
        &config_artifacts_store,
    )
    .await?;

    if artifact.prepare_only {
        print_prepare_summary(&build_store, &pre_lock_digests, selected_artifact_digest);

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
        output_path = get_artifact_output_path(selected_artifact_digest, &artifact.namespace)
            .display()
            .to_string();
        &output_path
    } else {
        selected_artifact_digest
    };

    build_artifacts(
        &artifact.namespace,
        Some(selected_artifact),
        artifact.aliases,
        build_store,
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
}
