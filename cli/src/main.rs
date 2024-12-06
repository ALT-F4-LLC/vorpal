use crate::worker::build;
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use console::style;
use port_selector::random_free_port;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::{process, process::Child};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::transport::{Channel, Server};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use vorpal_registry::service::RegistryServer;
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{
            artifact_service_server::ArtifactServiceServer, ArtifactId, ArtifactSystem,
            ArtifactSystem::UnknownSystem,
        },
        config::v0::config_service_client::ConfigServiceClient,
        registry::v0::registry_service_server::RegistryServiceServer,
    },
};
use vorpal_store::paths::{get_private_key_path, get_public_key_path, setup_paths};
use vorpal_worker::artifact::ArtifactServer;

mod build;
mod log;
mod worker;

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(long, short)]
        artifact: String,

        #[arg(long, short)]
        file: String,

        #[clap(default_value = "http://localhost:23151", long, short)]
        registry: String,

        #[arg(default_value_t = get_default_system(), long, short)]
        system: String,

        #[clap(default_value = "http://localhost:23151", long, short)]
        worker: String,
    },

    Config {
        #[arg(long, short)]
        file: String,

        #[arg(long, short)]
        artifact: String,

        #[arg(default_value_t = get_default_system(), long, short)]
        system: String,
    },

    #[clap(subcommand)]
    Keys(CommandKeys),

    Start {
        #[clap(default_value = "http://localhost:23151", long, short)]
        artifact_registry: String,

        #[arg(default_value_t = Level::INFO, global = true, long)]
        level: Level,

        #[clap(default_value = "23151", long, short)]
        port: u16,

        #[arg(default_value = "artifact,registry", long, short)]
        services: String,
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

        println!("Config: {}", line);

        if line.contains("Config server listening on") {
            println!("{} {}", style("Config:").bold().green(), host);
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Build {
            artifact,
            file,
            registry,
            system,
            worker,
        } => {
            if worker.is_empty() {
                bail!("no worker specified");
            }

            let artifact_system: ArtifactSystem = get_artifact_system(system);

            if artifact_system == UnknownSystem {
                bail!("unknown target: {}", artifact_system.as_str_name());
            }

            setup_paths().await?;

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                bail!("private key not found - run 'vorpal keys generate' or copy from worker",);
            }

            let (mut config_process, mut config_service) = start_config(file.to_string()).await?;

            let (artifacts_map, artifacts_order) =
                build::load_config(artifact, &mut config_service).await?;

            let mut artifacts_ids = HashMap::<String, ArtifactId>::new();

            for artifact_id in &artifacts_order {
                match artifacts_map.get(artifact_id) {
                    None => bail!("Build artifact not found: {}", artifact_id.name),
                    Some(artifact) => {
                        for a in &artifact.artifacts {
                            if !artifacts_ids.contains_key(&a.name) {
                                bail!("Artifact not found: {}", a.name);
                            }
                        }

                        build(artifact, artifact_id, artifact_system, registry, worker).await?;

                        artifacts_ids.insert(artifact_id.name.clone(), artifact_id.clone());
                    }
                }
            }

            config_process
                .kill()
                .await
                .map_err(|_| anyhow!("failed to kill config server"))
        }

        Command::Config {
            file,
            artifact,
            system,
        } => {
            let artifact_system: ArtifactSystem = get_artifact_system(system);

            if artifact_system == UnknownSystem {
                bail!("unknown target: {}", artifact_system.as_str_name());
            }

            let (mut config_process, mut config_service) = start_config(file.to_string()).await?;

            let _ = build::load_config(artifact, &mut config_service).await?;

            config_process
                .kill()
                .await
                .map_err(|_| anyhow!("failed to kill config server"))
        }

        Command::Keys(keys) => match keys {
            CommandKeys::Generate {} => {
                let key_dir_path = vorpal_store::paths::get_key_dir_path();
                let private_key_path = vorpal_store::paths::get_private_key_path();
                let public_key_path = vorpal_store::paths::get_public_key_path();

                if private_key_path.exists() && public_key_path.exists() {
                    println!("=> Keys already exist: {}", key_dir_path.display());
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
            artifact_registry,
            level,
            port,
            services,
        } => {
            let mut subscriber = FmtSubscriber::builder().with_max_level(*level);

            if [Level::DEBUG, Level::TRACE].contains(level) {
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
                let service = ArtifactServiceServer::new(ArtifactServer::new(
                    artifact_registry.to_string(),
                    system,
                ));

                info!("artifact service: [::]:{}", port);

                router = router.add_service(service);
            }

            if services.contains("registry") {
                let service = RegistryServiceServer::new(RegistryServer::default());

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
