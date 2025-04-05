use crate::artifact::build;
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use console::style;
use std::collections::HashMap;
use tonic::transport::Server;
use tracing::{info, subscriber, warn, Level};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::FmtSubscriber;
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
        artifact_service_server::ArtifactServiceServer, Artifact, ArtifactRequest, ArtifactSystem,
        ArtifactsRequest,
    },
    system_default, system_default_str, system_from_str,
    worker::v0::{
        worker_service_client::WorkerServiceClient, worker_service_server::WorkerServiceServer,
    },
};
use vorpal_store::{notary::generate_keys, paths::get_public_key_path};
use vorpal_worker::artifact::WorkerServer;

mod artifact;
mod build;
mod config;

pub fn get_prefix(name: &str) -> String {
    style(format!("{} |>", name)).bold().to_string()
}

#[derive(Subcommand)]
enum Command {
    Artifact {
        #[arg(default_value_t = false, long)]
        export: bool,

        #[arg(long)]
        name: String,

        #[arg(default_value_t = system_default_str(), long)]
        target: String,
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
    #[clap(default_value = "http://localhost:23151", long, short)]
    agent: String,

    #[command(subcommand)]
    command: Command,

    #[arg(default_value_t = get_current_dir(), long)]
    config_path: String,

    #[arg(default_value = "rust", long)]
    language: String,

    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,

    #[clap(default_value = "http://localhost:23151", long, short)]
    registry: String,

    #[arg(default_value = "vorpal-config", long)]
    rust_bin: String,

    #[clap(default_value = "http://localhost:23151", long)]
    worker: String,
}

fn get_current_dir() -> String {
    std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let Cli {
        agent,
        command,
        config_path,
        language,
        level,
        registry,
        rust_bin,
        worker,
    } = cli;

    match &command {
        Command::Artifact {
            export: _artifact_export,
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

            // Get config

            let target = system_from_str(target)?;

            if target == ArtifactSystem::UnknownSystem {
                bail!("unsupported target: {}", target.as_str_name());
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

            // Start config

            let config_path = config::get_path(
                &agent,
                &config_path,
                &language,
                &registry,
                &mut registry_archive,
                &mut registry_artifact,
                &rust_bin,
                &target,
                &mut worker,
            )
            .await?;

            if !config_path.exists() {
                bail!("config file not found: {}", config_path.display());
            }

            let (mut config_process, mut config_artifact) =
                config::start(config_path.display().to_string(), registry.clone()).await?;

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

            // Populate artifacts

            let mut artifact_selected = HashMap::<String, Artifact>::new();

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

                artifact_selected.insert(digest, response.into_inner());
            }

            // Populate artifacts

            let (selected_hash, selected) = artifact_selected
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

            // Build artifacts

            config::build_artifacts(
                Some(&selected),
                artifact,
                &mut registry_archive,
                &mut registry_artifact,
                &mut worker,
            )
            .await?;

            config_process.kill().await?;

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
                let system = system_default()?;

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
