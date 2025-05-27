use crate::command::{
    artifact::config::{build_artifacts, get_artifacts, start},
    store::{
        archives::unpack_zstd,
        paths::{
            get_artifact_archive_path, get_artifact_output_path, get_file_paths, set_timestamps,
        },
    },
};
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::Path,
    process::{exit, Stdio},
};
use tokio::{
    fs::{create_dir_all, read, write},
    process::Command,
};
use toml::from_str;
use tonic::{transport::Channel, Code};
use tracing::{error, info, subscriber, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};
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

mod config;

#[derive(Deserialize)]
pub struct VorpalConfigGo {
    pub directory: Option<String>,
}

#[derive(Deserialize)]
pub struct VorpalConfigRust {
    pub bin: Option<String>,
    pub packages: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct VorpalConfigSource {
    pub includes: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct VorpalConfig {
    pub go: Option<VorpalConfigGo>,
    pub language: Option<String>,
    pub name: Option<String>,
    pub rust: Option<VorpalConfigRust>,
    pub source: Option<VorpalConfigSource>,
}

pub async fn build(
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

#[allow(clippy::too_many_arguments)]
pub async fn run(
    agent: &str,
    artifact_aliases: Vec<String>,
    artifact_config: &str,
    artifact_context: &str,
    artifact_export: bool,
    artifact_lockfile_update: bool,
    artifact_name: &str,
    artifact_path: bool,
    artifact_system: &str,
    level: Level,
    registry: &str,
    variable: Vec<String>,
    worker: &str,
) -> Result<()> {
    // Setup logging

    let subscriber_writer = std::io::stderr.with_max_level(level);

    let mut subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_writer(subscriber_writer)
        .without_time();

    if [Level::DEBUG, Level::TRACE].contains(&level) {
        subscriber = subscriber.with_file(true).with_line_number(true);
    }

    let subscriber = subscriber.finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber");

    // Setup configuration

    if artifact_config.is_empty() {
        error!("no `--config` specified");
        exit(1);
    }

    if artifact_context.is_empty() {
        error!("no `--context` specified");
        exit(1);
    }

    if artifact_name.is_empty() {
        error!("no `--name` specified");
        exit(1);
    }

    let artifact_context = Path::new(&artifact_context);

    if !artifact_context.exists() {
        error!("'context' not found: {}", artifact_context.display());
        exit(1);
    }

    let config_path = artifact_context.join(artifact_config);
    let config_data_bytes = read(config_path).await.expect("failed to read config");
    let config_data = String::from_utf8_lossy(&config_data_bytes);
    let config: VorpalConfig = from_str(&config_data).expect("failed to parse config");

    if config.language.is_none() {
        error!("no 'language' specified in Vorpal.yaml");
        exit(1);
    }

    let config_language = config.language.unwrap();
    let config_name = config.name.unwrap_or_else(|| "vorpal".to_string());

    // Build configuration

    let mut config_context = ConfigContext::new(
        agent.to_string(),
        config_name.to_string(),
        artifact_context.to_path_buf(),
        0,
        registry.to_string(),
        artifact_system.to_string(),
        variable.clone(),
    )?;

    let config_system = config_context.get_system();

    let config_digest = match config_language.as_str() {
        "go" => {
            let protoc = protoc::build(&mut config_context).await?;
            let protoc_gen_go = protoc_gen_go::build(&mut config_context).await?;
            let protoc_gen_go_grpc = protoc_gen_go_grpc::build(&mut config_context).await?;

            let source_path = format!("{}.go", config_name);

            let mut includes = vec![&source_path, "go.mod", "go.sum"];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            let mut builder = GoBuilder::new(&config_name, vec![config_system])
                .with_artifacts(vec![protoc, protoc_gen_go, protoc_gen_go_grpc])
                .with_includes(includes);

            if let Some(directory) = config.go.as_ref().and_then(|g| g.directory.as_ref()) {
                builder = builder.with_build_directory(directory.as_str());
            }

            builder.build(&mut config_context).await?
        }

        "rust" => {
            let mut bins = vec![config_name.as_str()];
            let bin_path = format!("src/{}.rs", config_name);
            let mut includes = vec![&bin_path, "Cargo.toml", "Cargo.lock"];
            let mut packages = vec![];

            if let Some(b) = config.rust.as_ref().and_then(|r| r.bin.as_ref()) {
                bins = vec![b.as_str()];
            }

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            if let Some(p) = config.rust.as_ref().and_then(|r| r.packages.as_ref()) {
                packages = p.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            RustBuilder::new(&config_name, vec![config_system])
                .with_bins(bins)
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

    let mut client_archive = ArchiveServiceClient::connect(registry.to_owned())
        .await
        .expect("failed to connect to registry");

    let mut client_worker = WorkerServiceClient::connect(worker.to_owned())
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
        config_name
    );

    let config_path = Path::new(&config_file);

    if !config_path.exists() {
        error!("config not found: {}", config_path.display());
        exit(1);
    }

    let (mut config_process, mut config_client) = match start(
        agent.to_string(),
        artifact_name.to_string(),
        artifact_context.to_path_buf(),
        artifact_lockfile_update,
        config_path.display().to_string(),
        registry.to_string(),
        artifact_system.to_string(),
        variable.clone(),
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

    let (artifact_digest, artifact) = config_artifacts_store
        .clone()
        .into_iter()
        .find(|(_, val)| val.name == *artifact_name)
        .ok_or_else(|| anyhow!("selected 'artifact' not found: {}", artifact_name))?;

    let mut build_store = HashMap::<String, Artifact>::new();

    get_artifacts(
        &artifact,
        &artifact_digest,
        &mut build_store,
        &config_artifacts_store,
    )
    .await?;

    if artifact_export {
        let export = serde_json::to_string_pretty(&artifact).expect("failed to serialize artifact");

        println!("{}", export);

        return Ok(());
    }

    build_artifacts(
        Some(&artifact),
        artifact_aliases,
        build_store,
        &mut client_archive,
        &mut client_worker,
    )
    .await?;

    // TODO: explore running post scripts

    let artifact_output_path = get_artifact_output_path(&artifact_digest);
    let mut output = artifact_digest.clone();

    if artifact_path {
        output = artifact_output_path.display().to_string();
    }

    println!("{output}");

    Ok(())
}
