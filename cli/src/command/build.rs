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
use anyhow::{anyhow, bail, Result};
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
            ArtifactsRequest,
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

    if !archive_path.exists() {
        let request = ArchivePullRequest {
            digest: artifact_digest.to_string(),
            namespace: artifact_namespace.to_string(),
        };

        let mut request = Request::new(request);
        let request_auth_header = client_auth_header(registry)
            .await
            .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

        if let Some(header) = request_auth_header {
            request.metadata_mut().insert("authorization", header);
        }

        match client_archive.pull(request).await {
            Err(status) => {
                if status.code() != Code::NotFound {
                    bail!("registry pull error: {:?}", status);
                }
            }

            Ok(response) => {
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
                                bail!("registry stream error: {:?}", status);
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

                    write(&archive_path, &stream_data)
                        .await
                        .expect("failed to write archive");

                    set_timestamps(&archive_path).await?;
                }
            }
        };
    }

    if archive_path.exists() {
        info!("{} |> unpack: {}", artifact.name, artifact_digest);

        create_dir_all(&artifact_path)
            .await
            .expect("failed to create artifact path");

        unpack_zstd(&artifact_path, &archive_path).await?;

        let artifact_files = get_file_paths(&artifact_path, vec![], vec![])?;

        if artifact_files.is_empty() {
            bail!("Artifact files not found: {:?}", artifact_path);
        }

        for artifact_files in &artifact_files {
            set_timestamps(artifact_files).await?;
        }

        return Ok(());
    }

    // Build

    let request = BuildArtifactRequest {
        artifact: Some(artifact.clone()),
        artifact_aliases,
        artifact_namespace: artifact_namespace.to_string(),
        registry: registry.to_string(),
    };

    let mut request = Request::new(request);
    let request_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

    if let Some(header) = request_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    let response = client_worker
        .build_artifact(request)
        .await
        .expect("failed to build");

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

        if !archive_path.exists() {
            let request = ArchivePullRequest {
                digest: artifact_digest.to_string(),
                namespace: artifact_namespace.to_string(),
            };

            let mut request = Request::new(request);
            let request_auth_header = client_auth_header(registry)
                .await
                .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

            if let Some(header) = request_auth_header {
                request.metadata_mut().insert("authorization", header);
            }

            match client_archive.pull(request).await {
                Err(status) => {
                    if status.code() != Code::NotFound {
                        bail!("registry pull error after build: {:?}", status);
                    }

                    info!(
                        "{} |> artifact has no output files (not found in registry)",
                        artifact.name
                    );
                }

                Ok(response) => {
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
                                    bail!("registry stream error after build: {:?}", status);
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

                        write(&archive_path, &stream_data)
                            .await
                            .expect("failed to write archive");

                        set_timestamps(&archive_path).await?;
                    }
                }
            }
        }

        if archive_path.exists() {
            info!("{} |> unpack: {}", artifact.name, artifact_digest);

            create_dir_all(&artifact_path)
                .await
                .expect("failed to create artifact path");

            unpack_zstd(&artifact_path, &archive_path).await?;

            let artifact_files = get_file_paths(&artifact_path, vec![], vec![])?;

            for artifact_file in &artifact_files {
                set_timestamps(artifact_file).await?;
            }
        }
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

    let mut build_complete = HashMap::<String, Artifact>::new();

    for artifact_digest in artifact_order {
        match build_store.get(&artifact_digest) {
            None => bail!("artifact 'config' not found: {}", artifact_digest),

            Some(artifact) => {
                for step in artifact.steps.iter() {
                    for hash in step.artifacts.iter() {
                        if !build_complete.contains_key(hash) {
                            bail!("artifact 'build' not found: {}", hash);
                        }
                    }
                }

                let mut artifact_aliases = vec![];

                if let Some(selected) = artifact_selected {
                    if selected.name == artifact.name {
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

                build_complete.insert(artifact_digest.to_string(), artifact.clone());

                // Sources are managed by the agent, no artifact entries needed
            }
        }
    }

    Ok(())
}

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

    let mut config_context = ConfigContext::new(
        config.name.to_string(),
        config.context.to_path_buf(),
        artifact.namespace.to_string(),
        resolve_config_system(artifact.prepare_only, &artifact.system),
        artifact.unlock,
        artifact.variable.clone(),
        client_agent,
        client_artifact,
        0,
        service.registry.clone(),
    )?;

    let config_system = config_context.get_system();

    let config_digest = match config.language.as_str() {
        "go" => {
            let protoc = Protoc::new().build(&mut config_context).await?;
            let protoc_gen_go = ProtocGenGo::new().build(&mut config_context).await?;
            let protoc_gen_go_grpc = ProtocGenGoGrpc::new().build(&mut config_context).await?;

            let source_path = format!("{}.go", config.name);

            let mut includes = vec![&source_path, "go.mod", "go.sum"];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            let mut builder = Go::new(&config.name, vec![config_system])
                .with_artifacts(vec![protoc, protoc_gen_go, protoc_gen_go_grpc])
                .with_includes(includes);

            if !config.environments.is_empty() {
                builder = builder
                    .with_environments(config.environments.iter().map(|s| s.as_str()).collect());
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

            builder.build(&mut config_context).await?
        }

        "rust" => {
            let mut bins = vec![config.name.to_string()];
            let bin_path = format!("src/{}.rs", config.name);
            let mut includes = vec![&bin_path, "Cargo.toml", "Cargo.lock"];
            let mut packages = vec![];

            if let Some(b) = config.source.as_ref().and_then(|s| s.rust.as_ref()) {
                if let Some(bin) = b.bin.as_ref() {
                    bins = vec![bin.to_string()];
                }

                if let Some(p) = b.packages.as_ref() {
                    packages = p.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
                }
            }

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            let mut builder = Rust::new(&config.name, vec![config_system])
                .with_bins(bins.iter().map(|s| s.as_str()).collect::<Vec<&str>>())
                .with_includes(includes)
                .with_packages(packages);

            if !config.environments.is_empty() {
                builder = builder
                    .with_environments(config.environments.iter().map(|s| s.as_str()).collect());
            }

            builder.build(&mut config_context).await?
        }

        "python" => {
            let entrypoint = config
                .source
                .as_ref()
                .and_then(|s| s.python.as_ref())
                .and_then(|p| p.entrypoint.as_ref())
                .map(|e| e.to_string())
                .unwrap_or_else(|| format!("src/{}.py", config.name));

            // Python projects are multi-file: include the package source tree, not just
            // the single entrypoint (the one deliberate divergence from the TypeScript arm).
            // README.md is required here (not in the other language arms) because hatchling
            // reads `[project].readme` at build time; omitting it fails `uv sync` for any
            // config relying on this default (DKT-30, following the DKT-28 workaround).
            let mut includes = vec!["pyproject.toml", "uv.lock", "src", "README.md"];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                if !i.is_empty() {
                    includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
                }
            }

            let mut builder = Python::new(&config.name, vec![config_system])
                .with_entrypoint(&entrypoint)
                .with_includes(includes);

            if !config.environments.is_empty() {
                builder = builder
                    .with_environments(config.environments.iter().map(|s| s.as_str()).collect());
            }

            let working_dir = config
                .source
                .as_ref()
                .and_then(|s| s.python.as_ref())
                .and_then(|p| p.directory.as_ref());

            if let Some(directory) = working_dir {
                builder = builder.with_working_dir(directory);
            }

            builder.build(&mut config_context).await?
        }

        "typescript" => {
            let entrypoint = config
                .source
                .as_ref()
                .and_then(|s| s.typescript.as_ref())
                .and_then(|t| t.entrypoint.as_ref())
                .map(|e| e.to_string())
                .unwrap_or_else(|| format!("src/{}.ts", config.name));

            let mut includes = vec![
                "bun.lock",
                "bun.lockb",
                "package.json",
                "tsconfig.json",
                &entrypoint,
            ];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                if !i.is_empty() {
                    includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
                }
            }

            let mut builder = TypeScript::new(&config.name, vec![config_system])
                .with_entrypoint(&entrypoint)
                .with_includes(includes);

            if !config.environments.is_empty() {
                builder = builder
                    .with_environments(config.environments.iter().map(|s| s.as_str()).collect());
            }

            let working_dir = config
                .source
                .as_ref()
                .and_then(|s| s.typescript.as_ref())
                .and_then(|t| t.directory.as_ref());

            if let Some(directory) = working_dir {
                builder = builder.with_working_dir(directory);
            }

            builder.build(&mut config_context).await?
        }

        other => {
            bail!(
                "Unsupported language '{}' in Vorpal.toml\n\n  \
                 Supported languages are: go, python, rust, typescript\n\n  \
                 To fix this, update the 'language' field in your Vorpal.toml:\n    \
                 language = \"typescript\"  # or \"python\", \"rust\", or \"go\"",
                other
            );
        }
    };

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

    let config_file = format!(
        "{}/bin/{}",
        &get_artifact_output_path(&config_digest, &artifact.namespace).display(),
        &config.name
    );

    let config_file = Path::new(&config_file);

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

    let (mut config_process, mut config_client) = match start(
        service.agent.to_string(),
        artifact.context.to_path_buf(),
        artifact.name.to_string(),
        artifact.namespace.to_string(),
        artifact.system.to_string(),
        artifact.unlock,
        artifact.variable.clone(),
        config_file.display().to_string(),
        service.registry.to_string(),
    )
    .await
    {
        Ok(res) => res,
        Err(error) => {
            error!("{}", error);
            exit(1);
        }
    };

    // Populate artifacts

    let config_artifacts_response = match config_client
        .get_artifacts(ArtifactsRequest {
            digests: vec![],
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

    for digest in config_artifacts_response.digests.into_iter() {
        let request = ArtifactRequest {
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

        let artifact = response.into_inner();

        config_artifacts_store.insert(digest, artifact);
    }

    config_process.kill().await?;

    let (selected_artifact_digest, selected_artifact) = config_artifacts_store
        .clone()
        .into_iter()
        .find(|(_, val)| val.name == artifact.name)
        .ok_or_else(|| anyhow!("selected 'artifact' not found: {}", artifact.name))?;

    if artifact.rebuild {
        // Remove artifact configuration output for rebuild

        let config_artifact_output_lock_path =
            get_artifact_output_lock_path(&config_digest, &artifact.namespace);

        if config_artifact_output_lock_path.exists() {
            remove_file(&config_artifact_output_lock_path)
                .await
                .expect("failed to remove config artifact lock file");
        }

        let config_artifact_output_path =
            get_artifact_output_path(&config_digest, &artifact.namespace);

        if config_artifact_output_path.exists() {
            remove_dir_all(&config_artifact_output_path)
                .await
                .expect("failed to remove config artifact path");
        }

        // Remove selected artifact output for rebuild

        let artifact_output_lock_path =
            get_artifact_output_lock_path(&selected_artifact_digest, &artifact.namespace);

        if artifact_output_lock_path.exists() {
            remove_file(&artifact_output_lock_path)
                .await
                .expect("failed to remove artifact lock file");
        }

        let artifact_output_path =
            get_artifact_output_path(&selected_artifact_digest, &artifact.namespace);

        if artifact_output_path.exists() {
            remove_dir_all(&artifact_output_path)
                .await
                .expect("failed to remove artifact path");
        }
    }

    let mut build_store = HashMap::<String, Artifact>::new();

    get_artifacts(
        &selected_artifact,
        &selected_artifact_digest,
        &mut build_store,
        &config_artifacts_store,
    )
    .await?;

    if artifact.prepare_only {
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
                        let key = (source.name.clone(), platform.clone());
                        let digest = source.digest.clone().unwrap_or_default();
                        let status =
                            classify_pin(pre_lock_digests.get(&key).map(String::as_str), &digest);

                        format!("{status}: {} ({}) -> {}", source.name, platform, digest)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        summary_lines.sort();
        summary_lines.dedup();

        for line in &summary_lines {
            println!("{line}");
        }

        println!("{selected_artifact_digest}");

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
                println!("{:<width$}  {}", a.name, digest, width = max_name_len);
            }
        }

        return Ok(());
    }

    if artifact.export {
        let export =
            serde_json::to_string_pretty(&selected_artifact).expect("failed to serialize artifact");

        println!("{export}");

        return Ok(());
    }

    build_artifacts(
        &artifact.namespace,
        Some(&selected_artifact),
        artifact.aliases,
        build_store,
        &mut client_archive,
        &mut client_worker,
        &service.registry,
    )
    .await?;

    // TODO: explore running post scripts

    let artifact_output_path =
        get_artifact_output_path(&selected_artifact_digest, &artifact.namespace);
    let mut output = selected_artifact_digest.clone();

    if artifact.path {
        output = artifact_output_path.display().to_string();
    }

    println!("{output}");

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
