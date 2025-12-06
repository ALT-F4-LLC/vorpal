use crate::command::{
    artifact::config::{get_artifacts, get_order, start},
    start::auth,
    store::{
        archives::unpack_zstd,
        paths::{
            get_artifact_archive_path, get_artifact_output_lock_path, get_artifact_output_path,
            get_file_paths, get_key_ca_path, set_timestamps,
        },
    },
    VorpalConfigSource,
};
use anyhow::{anyhow, bail, Result};
use http::uri::{InvalidUri, Uri};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::exit,
};
use tokio::fs::{create_dir_all, read, remove_dir_all, remove_file, write};
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig},
    Code, Request,
};
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
        language::{go::Go, rust::RustBuilder},
        protoc, protoc_gen_go, protoc_gen_go_grpc,
    },
    context::ConfigContext,
};

pub struct RunArgsArtifact {
    pub aliases: Vec<String>,
    pub context: PathBuf,
    pub export: bool,
    pub name: String,
    pub namespace: String,
    pub path: bool,
    pub rebuild: bool,
    pub system: String,
    pub unlock: bool,
    pub variable: Vec<String>,
}

pub struct RunArgsConfig {
    pub context: PathBuf,
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

    let request = ArchivePullRequest {
        digest: artifact_digest.to_string(),
        namespace: artifact_namespace.to_string(),
    };

    let mut request = Request::new(request);

    let client_auth_header = auth::client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

    if let Some(header) = client_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    // TODO: add check before pulling

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
                let archive_path = get_artifact_archive_path(artifact_digest, artifact_namespace);

                let archive_path_parent = archive_path
                    .parent()
                    .ok_or_else(|| anyhow!("failed to get archive parent path"))?;

                create_dir_all(archive_path_parent).await?;

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
        artifact_namespace: artifact_namespace.to_string(),
        registry: registry.to_string(),
    };

    let mut request = Request::new(request);

    let client_auth_header = auth::client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

    if let Some(header) = client_auth_header {
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

    let client_ca_pem_path = get_key_ca_path();
    let client_ca_pem = read(client_ca_pem_path).await?;
    let client_ca = Certificate::from_pem(client_ca_pem);

    let client_tls = ClientTlsConfig::new()
        .ca_certificate(client_ca)
        .domain_name("localhost");

    let client_agent_uri = service
        .agent
        .parse::<Uri>()
        .map_err(|e: InvalidUri| anyhow::anyhow!("invalid agent address: {}", e))?;

    let client_artifact_uri = service
        .registry
        .parse::<Uri>()
        .map_err(|e: InvalidUri| anyhow::anyhow!("invalid artifact address: {}", e))?;

    let client_agent_channel = Channel::builder(client_agent_uri)
        .tls_config(client_tls.clone())?
        .connect()
        .await?;

    let client_artifact_channel = Channel::builder(client_artifact_uri)
        .tls_config(client_tls.clone())?
        .connect()
        .await?;

    let client_agent = AgentServiceClient::new(client_agent_channel);
    let client_artifact = ArtifactServiceClient::new(client_artifact_channel);

    // let client_api_token = match api_token {
    //     Some(token) => token,
    //     None => {
    //         if let Ok(service_secret) = load_service_secret().await {
    //             service_secret
    //         } else {
    //             load_api_token_env()?
    //         }
    //     }
    // };

    // Prepare config context

    let mut config_context = ConfigContext::new(
        config.name.to_string(),
        config.context.to_path_buf(),
        artifact.namespace.to_string(),
        artifact.system.to_string(),
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
            let protoc = protoc::build(&mut config_context).await?;
            let protoc_gen_go = protoc_gen_go::build(&mut config_context).await?;
            let protoc_gen_go_grpc = protoc_gen_go_grpc::build(&mut config_context).await?;

            let source_path = format!("{}.go", config.name);

            let mut includes = vec![&source_path, "go.mod", "go.sum"];

            if let Some(i) = config.source.as_ref().and_then(|s| s.includes.as_ref()) {
                includes = i.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            }

            let mut builder = Go::new(&config.name, vec![config_system])
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

    let client_archive_uri = service
        .registry
        .parse::<Uri>()
        .map_err(|e: InvalidUri| anyhow::anyhow!("invalid archive address: {}", e))?;

    let client_archive_channel = Channel::builder(client_archive_uri)
        .tls_config(client_tls.clone())?
        .connect()
        .await?;

    let mut client_archive = ArchiveServiceClient::new(client_archive_channel);

    let client_worker_uri = service
        .worker
        .parse::<Uri>()
        .map_err(|e: InvalidUri| anyhow::anyhow!("invalid worker address: {}", e))?;

    let client_worker_channel = Channel::builder(client_worker_uri)
        .tls_config(client_tls.clone())?
        .connect()
        .await?;

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
        error!("config not found: {}", config_file.display());
        exit(1);
    }

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

    // Agent handles all lockfile operations internally
    let mode = if artifact.unlock {
        "unlocked"
    } else {
        "locked"
    };

    info!("mode: {}", mode);

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
