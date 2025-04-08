use crate::artifact::build;
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use console::style;
use std::{collections::HashMap, path::Path};
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
        artifact_service_client::ArtifactServiceClient,
        artifact_service_server::ArtifactServiceServer, Artifact, ArtifactRequest,
        ArtifactsRequest,
    },
    worker::v0::{
        worker_service_client::WorkerServiceClient, worker_service_server::WorkerServiceServer,
    },
};
use vorpal_sdk::system::{get_system_default, get_system_default_str};
use vorpal_store::{notary::generate_keys, paths::get_public_key_path};
use vorpal_worker::artifact::WorkerServer;

mod artifact;
mod build;
mod config;

#[derive(Subcommand)]
enum Command {
    Artifact {
        #[arg(default_value = "vorpal-config", long, short)]
        config: String,

        #[arg(default_value_t = false, long, short)]
        export: bool,

        #[arg(long, short)]
        name: String,

        #[arg(default_value_t = get_system_default_str(), long, short)]
        target: String,
    },

    #[clap(subcommand)]
    Keys(CommandKeys),

    Start {
        #[clap(default_value = "23151", long, short)]
        port: u16,

        #[arg(default_value = "agent,registry,worker", long, short)]
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
    #[clap(default_value = "http://localhost:23151", long, short)]
    agent: String,

    #[command(subcommand)]
    command: Command,

    #[arg(default_value_t = get_default_context(), long, short)]
    context: String,

    #[arg(default_value_t = Level::INFO, global = true, long, short)]
    level: Level,

    #[clap(default_value = "http://localhost:23151", long, short)]
    registry: String,

    #[clap(default_value = "http://localhost:23151", long, short)]
    worker: String,
}

fn get_default_context() -> String {
    std::env::current_dir()
        .unwrap_or_else(|_| Path::new(".").to_path_buf())
        .to_string_lossy()
        .to_string()
}

pub fn get_prefix(name: &str) -> String {
    style(format!("{} |>", name)).bold().to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let Cli {
        agent,
        command,
        context,
        level,
        registry,
        worker,
    } = cli;

    if context.is_empty() {
        bail!("no `--context` specified");
    }

    let context_path = Path::new(&context);

    if !context_path.exists() {
        bail!("context not found: {}", context_path.display());
    }

    match &command {
        Command::Artifact {
            config,
            export: artifact_export,
            name,
            target,
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

            // Setup config

            let config_file_path = format!("{}/{}", context, config.replace("./", ""));
            let config_path = Path::new(&config_file_path);

            if !config_path.exists() {
                bail!("config not found: {}", config_path.display());
            }

            // Start config

            let (mut config_process, mut config_artifact) = match config::start(
                config_path.display().to_string(),
                registry.clone(),
                target.to_string(),
            )
            .await
            {
                Ok(res) => res,
                Err(error) => {
                    error!("{}", error);
                    return Ok(());
                }
            };

            // Populate artifacts

            let config_response = match config_artifact
                .get_artifacts(ArtifactsRequest { digests: vec![] })
                .await
            {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to get config: {}", error);
                }
            };

            let config_response = config_response.into_inner();
            let mut config_result = HashMap::<String, Artifact>::new();

            for digest in config_response.digests.into_iter() {
                let request = ArtifactRequest {
                    digest: digest.clone(),
                };

                let response = match config_artifact.get_artifact(request).await {
                    Ok(res) => res,
                    Err(error) => {
                        bail!("failed to get artifact: {}", error);
                    }
                };

                let artifact = response.into_inner();

                config_result.insert(digest, artifact.clone());
            }

            // Setup clients

            let mut registry_archive = ArchiveServiceClient::connect(registry.to_owned())
                .await
                .expect("failed to connect to registry");

            let mut registry_artifact = ArtifactServiceClient::connect(registry.to_owned())
                .await
                .expect("failed to connect to registry");

            let mut worker = WorkerServiceClient::connect(worker.to_owned())
                .await
                .expect("failed to connect to artifact");

            // Populate artifacts

            let (selected_hash, selected) = config_result
                .clone()
                .into_iter()
                .find(|(_, artifact)| artifact.name == *name)
                .ok_or_else(|| anyhow!("selected 'artifact' not found: {}", name))?;

            let mut artifact = HashMap::<String, Artifact>::new();

            artifact.insert(selected_hash.to_string(), selected.clone());

            config::fetch_artifacts(
                &selected,
                &mut artifact,
                &mut config_artifact,
                &mut registry_artifact,
            )
            .await?;

            config_process.kill().await?;

            // Export artifact

            if *artifact_export {
                let artifacts = artifact.clone().into_values().collect::<Vec<Artifact>>();

                let artifact_json =
                    serde_json::to_string_pretty(&artifacts).expect("failed to serialize artifact");

                println!("{}", artifact_json);

                return Ok(());
            }

            // Build artifacts

            config::build_artifacts(
                Some(&selected),
                artifact,
                &mut registry_archive,
                &mut registry_artifact,
                &mut worker,
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
                let service = AgentServiceServer::new(AgentServer::new(agent.clone()));

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
