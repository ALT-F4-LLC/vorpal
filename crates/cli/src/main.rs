use crate::artifact::build;
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use console::style;
use serde::Deserialize;
use std::{collections::HashMap, path::Path};
use tokio::fs::read;
use toml::from_str;
use tonic::transport::Server;
use tracing::{error, info, subscriber, warn, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};
use vorpal_agent::service::AgentServer;
use vorpal_registry::{
    archive::{ArchiveBackend, ArchiveServer},
    artifact::{ArtifactBackend, ArtifactServer as RegistryArtifactServer},
    GhaBackend, LocalBackend, S3Backend, ServerBackend,
};
use vorpal_schema::{
    agent::v0::agent_service_server::AgentServiceServer,
    archive::v0::{
        archive_service_client::ArchiveServiceClient, archive_service_server::ArchiveServiceServer,
    },
    artifact::v0::{
        artifact_service_server::ArtifactServiceServer, Artifact, ArtifactRequest, ArtifactsRequest,
    },
    worker::v0::{
        worker_service_client::WorkerServiceClient, worker_service_server::WorkerServiceServer,
    },
};
use vorpal_sdk::{
    artifact::{
        language::{go::GoBuilder, rust::RustBuilder},
        protoc, protoc_gen_go, protoc_gen_go_grpc,
    },
    context::ConfigContext,
    system::{get_system_default, get_system_default_str},
};
use vorpal_store::{
    notary::generate_keys,
    paths::{get_public_key_path, get_store_path},
};
use vorpal_worker::artifact::WorkerServer;

mod artifact;
mod build;
mod config;

#[derive(Subcommand)]
enum Command {
    Artifact {
        #[clap(default_value = "http://localhost:23151", long)]
        agent: String,

        #[arg(default_value = "Vorpal.toml", long)]
        config: String,

        #[arg(default_value_t = false, long)]
        export: bool,

        #[arg(long)]
        name: String,

        #[arg(default_value_t = false, long)]
        path: bool,

        #[arg(default_value_t = get_system_default_str(), long)]
        system: String,

        #[arg(long)]
        variable: Vec<String>,

        #[clap(default_value = "http://localhost:23151", long)]
        worker: String,
    },

    #[clap(subcommand)]
    Keys(CommandKeys),

    Start {
        #[clap(default_value = "23151", long)]
        port: u16,

        #[arg(default_value = "agent,registry,worker", long)]
        services: String,

        #[arg(default_value = "local", long)]
        registry_backend: String,

        #[arg(long)]
        registry_backend_s3_bucket: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum CommandKeys {
    Generate {},
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Log level
    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,

    /// Registry address
    #[clap(default_value = "http://localhost:23151", long)]
    registry: String,
}

pub fn get_prefix(name: &str) -> String {
    style(format!("{} |>", name)).bold().to_string()
}

#[derive(Deserialize)]
pub struct VorpalConfigGoBuild {
    pub directory: Option<String>,
}

#[derive(Deserialize)]
pub struct VorpalConfigSource {
    pub includes: Vec<String>,
    pub script: Option<String>,
}

#[derive(Deserialize)]
pub struct VorpalConfigBuild {
    pub directory: Option<String>,
}

#[derive(Deserialize)]
pub struct VorpalConfig {
    pub build: Option<VorpalConfigBuild>,
    pub language: Option<String>,
    pub name: Option<String>,
    pub source: Option<VorpalConfigSource>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let Cli {
        command,
        level,
        registry,
    } = cli;

    match &command {
        Command::Artifact {
            agent,
            config,
            export: artifact_export,
            name: artifact_name,
            path: artifact_path,
            system: artifact_system,
            variable,
            worker,
        } => {
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

            if config.is_empty() {
                error!("no `--config` specified");
                std::process::exit(1);
            }

            let config_path = Path::new(&config);

            if !config_path.exists() {
                error!("config not found: {}", config_path.display());
                std::process::exit(1);
            }

            let config_data_bytes = read(config_path).await.expect("failed to read config");
            let config_data = String::from_utf8_lossy(&config_data_bytes);
            let config: VorpalConfig = from_str(&config_data).expect("failed to parse config");

            if config.language.is_none() {
                error!("no 'language' specified in Vorpal.yaml");
                std::process::exit(1);
            }

            let config_language = config.language.unwrap();
            let config_name = config.name.unwrap_or_else(|| "vorpal-config".to_string());

            // Build configuration

            let mut config_context = ConfigContext::new(
                agent.to_string(),
                config_name.to_string(),
                0,
                registry.to_string(),
                artifact_system.to_string(),
                variable.clone(),
            )?;

            let protoc = protoc::build(&mut config_context).await?;

            let config_digest = match config_language.as_str() {
                "go" => {
                    let protoc_gen_go = protoc_gen_go::build(&mut config_context).await?;
                    let protoc_gen_go_grpc = protoc_gen_go_grpc::build(&mut config_context).await?;
                    let artifacts = vec![protoc, protoc_gen_go, protoc_gen_go_grpc];

                    let mut builder = GoBuilder::new(&config_name).with_artifacts(artifacts);

                    if let Some(build) = config.build.as_ref() {
                        if let Some(directory) = build.directory.as_ref() {
                            builder = builder.with_build_directory(directory);
                        }
                    }

                    if let Some(source) = config.source.as_ref() {
                        if !source.includes.is_empty() {
                            builder = builder.with_includes(
                                source.includes.iter().map(|s| s.as_str()).collect(),
                            );
                        }

                        if let Some(script) = source.script.as_ref() {
                            builder = builder.with_source_script(script);
                        }
                    }

                    builder.build(&mut config_context).await?
                }

                "rust" => {
                    let mut builder = RustBuilder::new(&config_name)
                        .with_artifacts(vec![protoc])
                        .with_bins(vec![&config_name]);

                    if let Some(source) = config.source.as_ref() {
                        if !source.includes.is_empty() {
                            builder = builder.with_packages(
                                source.includes.iter().map(|s| s.as_str()).collect(),
                            );
                        }
                    }

                    builder.build(&mut config_context).await?
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

            config::build_artifacts(
                *artifact_path,
                None,
                config_context.get_artifact_store(),
                &mut client_archive,
                &mut client_worker,
            )
            .await?;

            // Start configuration

            let config_file = format!(
                "{}/bin/{}",
                &get_store_path(&config_digest).display(),
                config_name
            );

            let config_path = Path::new(&config_file);

            if !config_path.exists() {
                error!("config not found: {}", config_path.display());
                std::process::exit(1);
            }

            let (mut config_process, mut config_client) = match config::start(
                agent.to_string(),
                artifact_name.to_string(),
                config_path.display().to_string(),
                registry.clone(),
                artifact_system.to_string(),
                variable.clone(),
            )
            .await
            {
                Ok(res) => res,
                Err(error) => {
                    error!("{}", error);
                    std::process::exit(1);
                }
            };

            // Populate artifacts

            let config_response = match config_client
                .get_artifacts(ArtifactsRequest { digests: vec![] })
                .await
            {
                Ok(res) => res,
                Err(error) => {
                    error!("failed to get config: {}", error);
                    std::process::exit(1);
                }
            };

            let config_response = config_response.into_inner();
            let mut config_store = HashMap::<String, Artifact>::new();

            for digest in config_response.digests.into_iter() {
                let request = ArtifactRequest {
                    digest: digest.clone(),
                };

                let response = match config_client.get_artifact(request).await {
                    Ok(res) => res,
                    Err(error) => {
                        error!("failed to get artifact: {}", error);
                        std::process::exit(1);
                    }
                };

                let artifact = response.into_inner();

                config_store.insert(digest, artifact);
            }

            config_process.kill().await?;

            let (artifact_digest, artifact) = config_store
                .clone()
                .into_iter()
                .find(|(_, val)| val.name == *artifact_name)
                .ok_or_else(|| anyhow!("selected 'artifact' not found: {}", artifact_name))?;

            let mut build_store = HashMap::<String, Artifact>::new();

            config::get_artifacts(&artifact, &artifact_digest, &mut build_store, &config_store)
                .await?;

            if *artifact_export {
                let artifacts = build_store.clone().into_values().collect::<Vec<Artifact>>();

                let artifacts_json =
                    serde_json::to_string_pretty(&artifacts).expect("failed to serialize artifact");

                println!("{}", artifacts_json);

                return Ok(());
            }

            config::build_artifacts(
                *artifact_path,
                Some(&artifact),
                build_store,
                &mut client_archive,
                &mut client_worker,
            )
            .await?;

            Ok(())
        }

        Command::Keys(keys) => match keys {
            CommandKeys::Generate {} => {
                let key_dir_path = vorpal_store::paths::get_key_dir_path();
                let private_key_path = vorpal_store::paths::get_private_key_path();
                let public_key_path = vorpal_store::paths::get_public_key_path();

                if private_key_path.exists() && public_key_path.exists() {
                    warn!("Keys already exist: {}", key_dir_path.display());

                    return Ok(());
                }

                if private_key_path.exists() && !public_key_path.exists() {
                    bail!("private key exists but public key is missing");
                }

                if !private_key_path.exists() && public_key_path.exists() {
                    bail!("public key exists but private key is missing");
                }

                generate_keys(key_dir_path, private_key_path, public_key_path).await?;

                Ok(())
            }
        },

        Command::Start {
            port,
            registry_backend,
            registry_backend_s3_bucket,
            services,
        } => {
            let mut subscriber = FmtSubscriber::builder()
                .with_target(false)
                .without_time()
                .with_max_level(level);

            if [Level::DEBUG, Level::TRACE].contains(&level) {
                subscriber = subscriber.with_file(true).with_line_number(true);
            }

            let subscriber = subscriber.finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber");

            let public_key_path = get_public_key_path();

            if !public_key_path.exists() {
                return Err(anyhow::anyhow!(
                    "public key not found - run 'vorpal keys generate' or copy from agent"
                ));
            }

            let (_, health_service) = tonic_health::server::health_reporter();

            let mut router = Server::builder().add_service(health_service);

            if services.contains("agent") {
                let service = AgentServiceServer::new(AgentServer::new(registry.clone()));

                info!("agent service: [::]:{}", port);

                router = router.add_service(service);
            }

            if services.contains("registry") {
                let backend = match registry_backend.as_str() {
                    "gha" => ServerBackend::GHA,
                    "local" => ServerBackend::Local,
                    "s3" => ServerBackend::S3,
                    _ => ServerBackend::Unknown,
                };

                if backend == ServerBackend::Unknown {
                    bail!("unknown registry backend: {}", registry_backend);
                }

                if backend == ServerBackend::S3 && registry_backend_s3_bucket.is_none() {
                    bail!("s3 backend requires '--registry-backend-s3-bucket' parameter");
                }

                let backend_archive: Box<dyn ArchiveBackend> = match backend {
                    ServerBackend::Local => Box::new(LocalBackend::new()?),
                    ServerBackend::S3 => {
                        Box::new(S3Backend::new(registry_backend_s3_bucket.clone()).await?)
                    }
                    ServerBackend::GHA => Box::new(GhaBackend::new()?),
                    ServerBackend::Unknown => unreachable!(),
                };

                let backend_artifact: Box<dyn ArtifactBackend> = match backend {
                    ServerBackend::Local => Box::new(LocalBackend::new()?),
                    ServerBackend::S3 => {
                        Box::new(S3Backend::new(registry_backend_s3_bucket.clone()).await?)
                    }
                    ServerBackend::GHA => Box::new(GhaBackend::new()?),
                    ServerBackend::Unknown => unreachable!(),
                };

                info!("registry service: [::]:{}", port);

                router = router.add_service(ArchiveServiceServer::new(ArchiveServer::new(
                    backend_archive,
                )));

                router = router.add_service(ArtifactServiceServer::new(
                    RegistryArtifactServer::new(backend_artifact),
                ));
            }

            if services.contains("worker") {
                let system = get_system_default()?;

                let service =
                    WorkerServiceServer::new(WorkerServer::new(registry.to_owned(), system));

                info!("worker service: [::]:{}", port);

                router = router.add_service(service);
            }

            let address = format!("[::]:{}", port)
                .parse()
                .expect("failed to parse address");

            router
                .serve(address)
                .await
                .expect("failed to start worker server");

            Ok(())
        }
    }
}
