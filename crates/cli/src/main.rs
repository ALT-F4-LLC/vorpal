use crate::artifact::{build, build_source};
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use console::style;
use std::collections::HashMap;
use tonic::transport::Server;
use tracing::{info, subscriber, warn, Level};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::FmtSubscriber;
use vorpal_registry::{RegistryBackend, RegistryServer, RegistryServerBackend};
use vorpal_schema::{
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient,
        artifact_service_server::ArtifactServiceServer,
    },
    config::v0::{ConfigArtifact, ConfigArtifactRequest, ConfigArtifactSystem, ConfigRequest},
    registry::v0::{
        registry_service_client::RegistryServiceClient,
        registry_service_server::RegistryServiceServer,
    },
    system_default, system_default_str, system_from_str,
};
use vorpal_store::{notary::generate_keys, paths::get_public_key_path};
use vorpal_worker::artifact::ArtifactServer;

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

        #[clap(default_value = "http://localhost:23151", long)]
        service: String,

        #[arg(default_value_t = system_default_str(), long)]
        target: String,
    },

    #[clap(subcommand)]
    Keys(CommandKeys),

    Start {
        #[clap(default_value = "23151", long)]
        port: u16,

        #[arg(default_value = "artifact,registry", long)]
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
        command,
        config_path,
        language,
        level,
        registry,
        rust_bin,
    } = cli;

    match &command {
        Command::Artifact {
            export: _artifact_export,
            name,
            service,
            target,
        } => {
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

            if service.is_empty() {
                bail!("no `--artifact-service` specified");
            }

            // Get config

            let target = system_from_str(target)?;

            if target == ConfigArtifactSystem::UnknownSystem {
                bail!("unsupported target: {}", target.as_str_name());
            }

            let config_path = config::get_path(
                &config_path,
                language,
                &registry,
                &rust_bin,
                service,
                target,
            )
            .await?;

            if !config_path.exists() {
                bail!("config file not found: {}", config_path.display());
            }

            let (mut config_process, mut client_config) =
                config::start(config_path.display().to_string(), registry.clone()).await?;

            let config_response = match client_config.get_config(ConfigRequest {}).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to get config: {}", error);
                }
            };

            let config_response = config_response.into_inner();

            // Populate artifacts

            let mut artifact_selected = HashMap::<String, ConfigArtifact>::new();

            for hash in config_response.artifacts.into_iter() {
                let request = ConfigArtifactRequest { hash: hash.clone() };

                let response = match client_config.get_config_artifact(request).await {
                    Ok(res) => res,
                    Err(error) => {
                        bail!("failed to get artifact: {}", error);
                    }
                };

                artifact_selected.insert(hash, response.into_inner());
            }

            // Find artifact

            let (artifact_selected_hash, artifact_selected) = artifact_selected
                .clone()
                .into_iter()
                .find(|(_, artifact)| artifact.name == *name)
                .ok_or_else(|| anyhow!("selected 'artifact' not found: {}", name))?;

            // Fetch artifacts

            let mut client_registry = RegistryServiceClient::connect(registry.to_owned())
                .await
                .expect("failed to connect to registry");

            let mut artifact_pending = HashMap::<String, ConfigArtifact>::new();

            artifact_pending.insert(
                artifact_selected_hash.to_string(),
                artifact_selected.clone(),
            );

            config::fetch_artifacts(
                &artifact_selected,
                &mut artifact_pending,
                &mut client_config,
                &mut client_registry,
            )
            .await?;

            // Setup service

            let mut client_artifact = ArtifactServiceClient::connect(service.to_owned())
                .await
                .expect("failed to connect to artifact");

            // Build artifacts

            config::build_artifacts(
                Some(&artifact_selected),
                artifact_pending,
                &mut client_artifact,
                &mut client_registry,
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

            if services.contains("artifact") {
                let system = system_default()?;

                let service = ArtifactServiceServer::new(ArtifactServer::new(registry, system));

                info!("artifact service: [::]:{}", port);

                router = router.add_service(service);
            }

            if services.contains("registry") {
                let backend = match registry_backend.as_str() {
                    "gha" => RegistryServerBackend::GHA,
                    "local" => RegistryServerBackend::Local,
                    "s3" => RegistryServerBackend::S3,
                    _ => RegistryServerBackend::Unknown,
                };

                if backend == RegistryServerBackend::Unknown {
                    bail!("unknown registry backend: {}", registry_backend);
                }

                if backend == RegistryServerBackend::S3 && registry_backend_s3_bucket.is_none() {
                    bail!("s3 backend requires '--registry-backend-s3-bucket' parameter");
                }

                let backend: Box<dyn RegistryBackend> = match backend {
                    RegistryServerBackend::Local => {
                        Box::new(vorpal_registry::LocalRegistryBackend::new()?)
                    }
                    RegistryServerBackend::S3 => Box::new(
                        vorpal_registry::S3RegistryBackend::new(registry_backend_s3_bucket.clone())
                            .await?,
                    ),
                    RegistryServerBackend::GHA => {
                        Box::new(vorpal_registry::GhaRegistryBackend::new()?)
                    }
                    RegistryServerBackend::Unknown => unreachable!(),
                };

                let service = RegistryServiceServer::new(RegistryServer::new(backend));

                info!("registry service: [::]:{}", port);

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
