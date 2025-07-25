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
                )
                .await?;

                build_complete.insert(artifact_digest.to_string(), artifact.clone());
            }
        }
    }

    Ok(())
}

pub struct RunArgsArtifact {
    pub aliases: Vec<String>,
    pub context: PathBuf,
    pub export: bool,
    pub lockfile_update: bool,
    pub name: String,
    pub path: bool,
    pub rebuild: bool,
    pub system: String,
    pub variable: Vec<String>,
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
            let bin_path = format!("src/{}.rs", config_name);
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

    let mut client_archive = ArchiveServiceClient::connect(service.registry.to_owned())
        .await
        .expect("failed to connect to registry");

    let mut client_worker = WorkerServiceClient::connect(service.worker.to_owned())
        .await
        .expect("failed to connect to artifact");

    build_artifacts(
        None,
        vec![],
        config_context.get_artifact_store(),
        &mut client_archive,
        &mut client_worker,
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
        artifact.lockfile_update,
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

    if artifact.export {
        let export =
            serde_json::to_string_pretty(&selected_artifact).expect("failed to serialize artifact");

        println!("{}", export);

        return Ok(());
    }

    build_artifacts(
        Some(&selected_artifact),
        artifact.aliases,
        build_store,
        &mut client_archive,
        &mut client_worker,
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
