use crate::artifact::build;
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use port_selector::random_free_port;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::{process, process::Child};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::transport::{Channel, Server};
use tracing::{info, warn, Level};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::FmtSubscriber;
use vorpal_registry::{RegistryServer, RegistryServerBackend};
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{
            artifact_service_server::ArtifactServiceServer, ArtifactId, ArtifactSystem,
            ArtifactSystem::UnknownSystem,
        },
        config::v0::{config_service_client::ConfigServiceClient, ConfigRequest},
        registry::v0::registry_service_server::RegistryServiceServer,
    },
};
use vorpal_store::paths::{get_artifact_path, get_public_key_path, setup_paths};

use vorpal_worker::artifact::ArtifactServer;

mod artifact;
mod build;

#[derive(Subcommand)]
enum Command {
    Artifact {
        #[arg(long)]
        name: String,

        #[clap(default_value = "http://localhost:23151", long)]
        service: String,

        #[arg(default_value_t = get_default_system(), long)]
        system: String,
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

    #[arg(default_value = "Vorpal.toml", long, short)]
    config: String,

    #[arg(default_value = "rust", long)]
    language: String,

    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,

    #[clap(default_value = "http://localhost:23151", long, short)]
    registry: String,

    #[arg(default_value = "vorpal-config", long)]
    rust_bin: Option<String>,

    #[arg(default_value = ".", long)]
    rust_path: Option<String>,
}

fn get_default_system() -> String {
    format!("{}-{}", ARCH, OS)
}

async fn start_config(file: String) -> Result<(Child, ConfigServiceClient<Channel>)> {
    let port = random_free_port().ok_or_else(|| anyhow!("failed to find free port"))?;

    let mut command = process::Command::new(file);

    command.args(["start", "--port", &port.to_string()]);

    let mut process = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| anyhow!("failed to start config server"))?;

    let stdout = process.stdout.take().unwrap();
    let stderr = process.stderr.take().unwrap();

    let stdout = LinesStream::new(BufReader::new(stdout).lines());
    let stderr = LinesStream::new(BufReader::new(stderr).lines());

    let mut stdio_merged = StreamExt::merge(stdout, stderr);

    let host = format!("http://localhost:{:?}", port);

    while let Some(line) = stdio_merged.next().await {
        let line = line.map_err(|err| anyhow!("failed to read line: {:?}", err))?;

        if !line.contains("Config listening") {
            info!("{}", line);
        }

        if line.contains("Config listening") {
            break;
        }
    }

    let service = match ConfigServiceClient::connect(host).await {
        Ok(srv) => srv,
        Err(e) => {
            let _ = process
                .kill()
                .await
                .map_err(|_| anyhow!("failed to kill config server"));

            bail!("failed to connect to config server: {}", e);
        }
    };

    Ok((process, service))
}

pub struct VorpalTomlLanguage {
    pub name: String,
}

pub struct VorpalTomlRust {
    pub bin: String,
    pub path: String,
}

pub struct VorpalToml {
    pub language: VorpalTomlLanguage,
    pub rust: VorpalTomlRust,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let Cli {
        command,
        config: _,
        language,
        level,
        registry,
        rust_bin,
        rust_path,
    } = cli;

    match &command {
        Command::Artifact {
            name,
            service,
            system,
        } => {
            let stderr_writer = std::io::stderr.with_max_level(level);

            let mut subscriber = FmtSubscriber::builder()
                .with_max_level(level)
                .with_target(false)
                .with_writer(stderr_writer)
                .without_time();

            if [Level::DEBUG, Level::TRACE].contains(&level) {
                subscriber = subscriber.with_file(true).with_line_number(true);
            }

            let subscriber = subscriber.finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber");

            // Build the configuration

            let config_file = match language.as_str() {
                "rust" => {
                    if rust_bin.is_none() {
                        bail!("no `--rust-bin` specified");
                    }

                    if rust_path.is_none() {
                        bail!("no `--rust-path` specified");
                    }

                    info!(
                        "building configuration... (cargo build --bin {} --release)",
                        rust_bin.as_ref().unwrap()
                    );

                    let cargo_bin =
                        which::which("cargo").map_err(|_| anyhow!("cargo not found in $PATH"))?;

                    let mut command = process::Command::new(cargo_bin);

                    let config_bin = rust_bin.as_ref().unwrap();

                    command.args(["build", "--bin", &config_bin, "--release"]);

                    let mut process = command
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                        .map_err(|_| anyhow!("failed to start config server"))?;

                    let stdout = process.stdout.take().unwrap();
                    let stderr = process.stderr.take().unwrap();

                    let stdout = LinesStream::new(BufReader::new(stdout).lines());
                    let stderr = LinesStream::new(BufReader::new(stderr).lines());

                    let mut stdio_merged = StreamExt::merge(stdout, stderr);

                    while let Some(line) = stdio_merged.next().await {
                        let line = line.map_err(|err| anyhow!("failed to read line: {:?}", err))?;

                        if !line.contains("Config listening") {
                            info!("{}", line);
                        }

                        if line.contains("Config listening") {
                            break;
                        }
                    }

                    info!("build complete");

                    let target_path = format!(
                        "{}/target/release/{}",
                        rust_path.as_ref().unwrap(),
                        config_bin
                    );

                    Path::new(&target_path).to_path_buf()
                }

                _ => bail!("unsupported language: {}", language),
            };

            if !config_file.exists() {
                bail!("config file not found: {}", config_file.display());
            }

            if service.is_empty() {
                bail!("no `--artifact-service` specified");
            }

            let artifact_system: ArtifactSystem = get_artifact_system(system);

            if artifact_system == UnknownSystem {
                bail!("unknown target: {}", artifact_system.as_str_name());
            }

            let (mut config_process, mut config_service) =
                start_config(config_file.display().to_string()).await?;

            let config_response = match config_service.get_config(ConfigRequest {}).await {
                Ok(res) => res,
                Err(error) => {
                    bail!("failed to evaluate config: {}", error);
                }
            };

            let config = config_response.into_inner();

            let config_artifact = config
                .artifacts
                .into_iter()
                .find(|a| a.name == *name)
                .ok_or_else(|| anyhow!("artifact not found: {}", name))?;

            let (build_map, build_order) =
                build::load_artifact(&config_artifact, &mut config_service).await?;

            let mut artifacts_ids = HashMap::<String, ArtifactId>::new();

            info!("building artifact... ({})", name);

            for artifact_id in &build_order {
                match build_map.get(artifact_id) {
                    None => bail!("Build artifact not found: {}", artifact_id.name),
                    Some(artifact) => {
                        for a in &artifact.artifacts {
                            if !artifacts_ids.contains_key(&a.name) {
                                bail!("Artifact not found: {}", a.name);
                            }
                        }

                        build(artifact, artifact_id, artifact_system, &registry, &service).await?;

                        artifacts_ids.insert(artifact_id.name.clone(), artifact_id.clone());

                        if artifact_id.name == *name {
                            println!(
                                "{}",
                                get_artifact_path(&artifact_id.hash, &artifact_id.name).display()
                            );
                        }
                    }
                }
            }

            info!("build complete");

            info!("shutting down config server...");

            let _ = config_process
                .kill()
                .await
                .map_err(|_| anyhow!("failed to kill config server"));

            info!("config server shut down");

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

                vorpal_notary::generate_keys(key_dir_path, private_key_path, public_key_path)
                    .await?;

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

            setup_paths().await?;

            let public_key_path = get_public_key_path();

            if !public_key_path.exists() {
                return Err(anyhow::anyhow!(
                    "public key not found - run 'vorpal keys generate' or copy from agent"
                ));
            }

            let (_, health_service) = tonic_health::server::health_reporter();

            let mut router = Server::builder().add_service(health_service);

            if services.contains("artifact") {
                let system = get_artifact_system(format!("{}-{}", ARCH, OS).as_str());
                let service = ArtifactServiceServer::new(ArtifactServer::new(registry, system));

                info!("artifact service: [::]:{}", port);

                router = router.add_service(service);
            }

            if services.contains("registry") {
                let backend = match registry_backend.as_str() {
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

                let service = RegistryServiceServer::new(RegistryServer::new(
                    backend,
                    registry_backend_s3_bucket.clone(),
                ));

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
