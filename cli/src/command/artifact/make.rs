use crate::command::lock::{load_lock, LockArtifact, LockSource, Lockfile};
use crate::command::{
    artifact::config::{get_artifacts, get_order, start},
    store::{
        archives::unpack_zstd,
        paths::{
            get_artifact_archive_path, get_artifact_output_lock_path, get_artifact_output_path,
            get_file_paths, set_timestamps,
        },
    },
    VorpalTomlConfigSource,
};
use anyhow::{anyhow, bail, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::exit,
};
use tokio::fs::{create_dir_all, remove_dir_all, remove_file, write};
use tonic::{transport::Channel, Code};
use tracing::{error, info};
use vorpal_sdk::{
    api::{
        archive::{archive_service_client::ArchiveServiceClient, ArchivePullRequest},
        artifact::{Artifact, ArtifactRequest, ArtifactsRequest},
        worker::{worker_service_client::WorkerServiceClient, BuildArtifactRequest},
    },
    artifact::{
        language::{go::GoBuilder, rust::RustBuilder},
        protoc, protoc_gen_go, protoc_gen_go_grpc,
    },
    context::ConfigContext,
};

async fn build(
    artifact: &Artifact,
    artifact_aliases: Vec<String>,
    artifact_digest: &str,
    client_archive: &mut ArchiveServiceClient<Channel>,
    client_worker: &mut WorkerServiceClient<Channel>,
    offline: bool,
) -> Result<()> {
    // 1. Check artifact

    let artifact_path = get_artifact_output_path(artifact_digest);

    if artifact_path.exists() {
        return Ok(());
    }

    // 2. Pull

    let request_pull = ArchivePullRequest {
        digest: artifact_digest.to_string(),
    };

    if !offline {
        match client_archive.pull(request_pull.clone()).await {
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
                    let archive_path = get_artifact_archive_path(artifact_digest);

                    write(&archive_path, &stream_data)
                        .await
                        .expect("failed to write archive");

                    set_timestamps(&archive_path).await?;

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
            }
        }
    };

    // Build

    let request = BuildArtifactRequest {
        artifact: Some(artifact.clone()),
        artifact_aliases,
    };

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

    Ok(())
}

async fn build_artifacts(
    artifact_selected: Option<&Artifact>,
    artifact_selected_aliases: Vec<String>,
    build_store: HashMap<String, Artifact>,
    client_archive: &mut ArchiveServiceClient<Channel>,
    client_worker: &mut WorkerServiceClient<Channel>,
    offline: bool,
    lock_path: &Path,
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
                    client_archive,
                    client_worker,
                    offline,
                )
                .await?;

                build_complete.insert(artifact_digest.to_string(), artifact.clone());

                // Incrementally append artifact entry to Vorpal.lock if missing (agent handles sources)
                upsert_artifact_lock_entry(lock_path, artifact, &artifact_digest).await?;
            }
        }
    }

    Ok(())
}

async fn upsert_artifact_lock_entry(lock_path: &Path, art: &Artifact, digest: &str) -> Result<()> {
    let mut lock = match load_lock(lock_path).await? {
        Some(l) => l,
        None => Lockfile {
            lockfile: 1,
            sources: vec![],
            artifacts: vec![],
        },
    };

    // If an entry with the same name already exists, do not modify it here (append-only semantics)
    if lock.artifacts.iter().any(|a| a.name == art.name) {
        return Ok(());
    }

    let deps = art
        .steps
        .iter()
        .flat_map(|s| s.artifacts.clone())
        .collect::<Vec<String>>();

    let systems = art
        .systems
        .iter()
        .map(|s| {
            vorpal_sdk::api::artifact::ArtifactSystem::try_from(*s)
                .map(|v| v.as_str_name().to_string())
                .unwrap_or_else(|_| s.to_string())
        })
        .collect::<Vec<String>>();

    lock.artifacts.push(LockArtifact {
        name: art.name.clone(),
        digest: digest.to_string(),
        aliases: art.aliases.clone(),
        systems,
        deps,
    });

    lock.artifacts
        .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

    crate::command::lock::save_lock_coordinated(lock_path, &lock).await?;
    Ok(())
}

pub struct RunArgsArtifact {
    pub aliases: Vec<String>,
    pub context: PathBuf,
    pub export: bool,
    pub offline: bool,
    pub update: bool,
    pub name: String,
    pub path: bool,
    pub rebuild: bool,
    pub system: String,
    pub variable: Vec<String>,
    pub verify: bool,
}

pub struct RunArgsConfig {
    pub context: PathBuf,
    pub language: String,
    pub name: String,
    pub source: Option<VorpalTomlConfigSource>,
}

pub struct RunArgsService {
    pub agent: String,
    pub registry: String,
    pub worker: String,
}

pub async fn run(
    artifact: RunArgsArtifact,
    config: RunArgsConfig,
    service: RunArgsService,
) -> Result<()> {
    // If verify flag is set, delegate to agent for verification
    if artifact.verify {
        bail!("--verify functionality moved to agent; use normal build process");
    }

    // Build configuration

    let artifact_system = &artifact.system;
    let config_name = &config.name;
    let service_agent = &service.agent;
    let service_registry = &service.registry;

    let mut config_context = ConfigContext::new(
        service_agent.to_string(),
        config_name.to_string(),
        config.context.to_path_buf(),
        0,
        service_registry.to_string(),
        artifact_system.to_string(),
        artifact.update,
        artifact.variable.clone(),
    )?;

    let config_system = config_context.get_system();

    let config_digest = match config.language.as_str() {
        "go" => {
            let protoc = protoc::build(&mut config_context).await?;
            let protoc_gen_go = protoc_gen_go::build(&mut config_context).await?;
            let protoc_gen_go_grpc = protoc_gen_go_grpc::build(&mut config_context).await?;

            let source_path = format!("{}.go", config.name);

            let mut includes = vec![&source_path, "go.mod", "go.sum"];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            let mut builder = GoBuilder::new(&config.name, vec![config_system])
                .with_artifacts(vec![protoc, protoc_gen_go, protoc_gen_go_grpc])
                .with_includes(includes);

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
            let mut bins = vec![config_name];
            let bin_path = format!("src/{config_name}.rs");
            let mut includes = vec![&bin_path, "Cargo.toml", "Cargo.lock"];
            let mut packages = vec![];

            if let Some(b) = config.source.as_ref().and_then(|s| s.rust.as_ref()) {
                if let Some(bin) = b.bin.as_ref() {
                    bins = vec![bin];
                }

                if let Some(p) = b.packages.as_ref() {
                    packages = p.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
                }
            }

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            RustBuilder::new(&config.name, vec![config_system])
                .with_bins(bins.iter().map(|s| s.as_str()).collect::<Vec<&str>>())
                .with_includes(includes)
                .with_packages(packages)
                .build(&mut config_context)
                .await?
        }

        _ => "".to_string(),
    };

    if config_digest.is_empty() {
        bail!("no config digest found");
    }

    // Prepare lock path early for incremental artifact updates
    let lock_path = artifact.context.join("Vorpal.lock");

    let mut client_archive = ArchiveServiceClient::connect(service.registry.to_owned())
        .await
        .expect("failed to connect to registry");

    let mut client_worker = WorkerServiceClient::connect(service.worker.to_owned())
        .await
        .expect("failed to connect to artifact");

    // Build config dependencies first to ensure config binary exists
    let config_store = config_context.get_artifact_store();
    build_artifacts(
        None,
        vec![],
        config_store,
        &mut client_archive,
        &mut client_worker,
        artifact.offline,
        &lock_path,
    )
    .await?;

    // Start configuration

    let config_file = format!(
        "{}/bin/{}",
        &get_artifact_output_path(&config_digest).display(),
        &config.name
    );

    let config_file = Path::new(&config_file);

    if !config_file.exists() {
        error!("config not found: {}", config_file.display());
        exit(1);
    }

    let (mut config_process, mut config_client) = match start(
        artifact.context.to_path_buf(),
        artifact.update,
        artifact.name.to_string(),
        artifact.system.to_string(),
        artifact.variable.clone(),
        config_file.display().to_string(),
        service.agent.to_string(),
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
        .get_artifacts(ArtifactsRequest { digests: vec![] })
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
        let artifact_output_lock_path = get_artifact_output_lock_path(&selected_artifact_digest);

        if artifact_output_lock_path.exists() {
            remove_file(&artifact_output_lock_path)
                .await
                .expect("failed to remove artifact lock file");
        }

        let artifact_output_path = get_artifact_output_path(&selected_artifact_digest);

        if artifact_output_path.exists() {
            remove_dir_all(&artifact_output_path)
                .await
                .expect("failed to remove artifact path");
        }

        info!("rebuilding artifact: {}", selected_artifact.name);
    }

    let mut build_store = HashMap::<String, Artifact>::new();

    get_artifacts(
        &selected_artifact,
        &selected_artifact_digest,
        &mut build_store,
        &config_artifacts_store,
    )
    .await?;

    // Update Vorpal.lock (artifacts only); sources are agent-managed mid-run

    let mut new_lock = Lockfile {
        lockfile: 1,
        sources: vec![],
        artifacts: vec![],
    };

    for (digest, art) in build_store.iter() {
        let deps = art
            .steps
            .iter()
            .flat_map(|s| s.artifacts.clone())
            .collect::<Vec<String>>();

        let systems = art
            .systems
            .iter()
            .map(|s| {
                vorpal_sdk::api::artifact::ArtifactSystem::try_from(*s)
                    .map(|v| v.as_str_name().to_string())
                    .unwrap_or_else(|_| s.to_string())
            })
            .collect::<Vec<String>>();

        // Policy: only lock artifacts that do not include local sources
        let has_local_source = art.sources.iter().any(|src| {
            let is_http = src.path.starts_with("http://") || src.path.starts_with("https://");
            !is_http
        });

        if !has_local_source {
            new_lock.artifacts.push(LockArtifact {
                name: art.name.clone(),
                digest: digest.clone(),
                aliases: art.aliases.clone(),
                systems,
                deps,
            });
        }

        // Do not persist sources here; agent updates Vorpal.lock per source as they finish
    }

    // Sort artifacts for deterministic output
    new_lock
        .artifacts
        .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

    let mut lock_status = "unchanged".to_string();

    if let Some(existing) = load_lock(&lock_path).await? {
        let mut ex = existing.clone();
        ex.artifacts
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

        // Build expected remote sources set from this run (artifact, name, url)
        let expected_sources: std::collections::HashSet<(String, String, String)> = build_store
            .iter()
            .flat_map(|(_, art)| {
                art.sources
                    .iter()
                    .filter(|s| s.path.starts_with("http://") || s.path.starts_with("https://"))
                    .map(|s| (art.name.clone(), s.name.clone(), s.path.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();

        // Determine current system string
        let current_system = new_lock
            .artifacts
            .iter()
            .find(|a| a.name == selected_artifact.name)
            .and_then(|a| a.systems.first().cloned())
            .unwrap_or_else(|| config_system.as_str_name().to_string());

        // Map artifact name -> systems for filtering
        let systems_by_artifact: std::collections::HashMap<String, Vec<String>> = new_lock
            .artifacts
            .iter()
            .map(|a| (a.name.clone(), a.systems.clone()))
            .collect();

        // Prune existing sources only for current system: keep others intact
        // Work on a cloned copy of sources to avoid moving out of `ex`
        let mut pruned_sources: Vec<LockSource> = ex
            .sources
            .clone()
            .into_iter()
            .filter(|s| s.kind != "local")
            .filter(|s| {
                // Sources without artifact binding are kept
                let Some(art) = &s.artifact else { return true };
                // If artifact isn't part of this run's artifacts, keep it
                let Some(art_systems) = systems_by_artifact.get(art) else {
                    return true;
                };
                // If this artifact does not target current system, keep it
                if !art_systems.contains(&current_system) {
                    return true;
                }
                // If this source is expected for current run, keep it; else prune
                expected_sources.contains(&(
                    art.clone(),
                    s.name.clone(),
                    s.url.clone().unwrap_or_default(),
                ))
            })
            .collect();

        pruned_sources.sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

        // Compose lock to save: merge existing artifacts with new ones, pruned sources
        let mut merged_artifacts = ex.artifacts.clone();

        // Update or add artifacts from the current build
        for new_artifact in &new_lock.artifacts {
            if let Some(existing_idx) = merged_artifacts
                .iter()
                .position(|a| a.name == new_artifact.name)
            {
                // Update existing artifact
                merged_artifacts[existing_idx] = new_artifact.clone();
            } else {
                // Add new artifact
                merged_artifacts.push(new_artifact.clone());
            }
        }

        let lock_to_save = Lockfile {
            lockfile: ex.lockfile,
            artifacts: merged_artifacts,
            sources: pruned_sources,
        };

        // Determine changes: artifacts vs sources separately
        // Locked policy: allow append-only changes to artifacts; modifying/removing existing entries fails.
        let mut artifacts_append_only_ok = true;
        use std::collections::HashMap;
        let mut ex_art_by_name: HashMap<&str, &LockArtifact> = HashMap::new();
        for a in &ex.artifacts {
            ex_art_by_name.insert(a.name.as_str(), a);
        }
        // All existing artifacts must be present identically in the new set
        for a in &ex.artifacts {
            if let Some(n) = lock_to_save.artifacts.iter().find(|na| na.name == a.name) {
                if n != a {
                    artifacts_append_only_ok = false; // digest/aliases/systems/deps changed
                }
            } else {
                artifacts_append_only_ok = false; // removal
            }
        }
        // If the new set differs at all, it's a change; may still be allowed if append-only
        let artifacts_changed = ex.artifacts != lock_to_save.artifacts;
        let sources_changed = ex.sources != lock_to_save.sources;

        // In locked mode, check if the changes are acceptable:
        // 1. All existing artifacts that should be locked are preserved identically
        // 2. Only artifacts with local sources (which shouldn't be locked) can be removed
        let acceptable_changes = ex.artifacts.iter().all(|existing| {
            // Check if this artifact should be locked (doesn't have local sources)
            // We need to find the artifact definition to check its sources
            let should_be_locked = build_store.iter().any(|(_, art)| {
                art.name == existing.name
                    && !art.sources.iter().any(|src| {
                        let is_http =
                            src.path.starts_with("http://") || src.path.starts_with("https://");
                        !is_http
                    })
            });

            if should_be_locked {
                // This artifact should be locked, so it must be preserved identically
                lock_to_save
                    .artifacts
                    .iter()
                    .any(|new| new.name == existing.name && new == existing)
            } else {
                // This artifact has local sources and can be removed from lockfile
                true
            }
        });

        if artifacts_changed && !artifact.update && !artifacts_append_only_ok && !acceptable_changes
        {
            bail!("Vorpal.lock would change; run with --update to refresh");
        }

        if artifacts_changed || sources_changed || artifact.update {
            crate::command::lock::save_lock_coordinated(&lock_path, &lock_to_save).await?;
            lock_status = "updated".to_string();
            info!("updated lockfile: {}", lock_path.display());
        }
    } else {
        // First run bootstrap: create minimal lockfile with artifacts; sources are agent-managed
        // Create minimal lockfile with artifacts; sources will be added by agent during runs
        crate::command::lock::save_lock_coordinated(&lock_path, &new_lock).await?;
        lock_status = "created".to_string();
        info!("created lockfile: {}", lock_path.display());
    }

    // Mode banner
    let mode = if artifact.update {
        "update"
    } else if artifact.offline {
        "ensure-offline"
    } else {
        "ensure"
    };
    info!("mode: {}, lock: {}", mode, lock_status);

    if artifact.export {
        let export =
            serde_json::to_string_pretty(&selected_artifact).expect("failed to serialize artifact");

        println!("{export}");

        return Ok(());
    }

    build_artifacts(
        Some(&selected_artifact),
        artifact.aliases,
        build_store,
        &mut client_archive,
        &mut client_worker,
        artifact.offline,
        &lock_path,
    )
    .await?;

    // TODO: explore running post scripts

    let artifact_output_path = get_artifact_output_path(&selected_artifact_digest);
    let mut output = selected_artifact_digest.clone();

    if artifact.path {
        output = artifact_output_path.display().to_string();
    }

    println!("{output}");

    Ok(())
}
