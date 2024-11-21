use crate::{log::connector_end, worker::build};
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use port_selector::random_free_port;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use tokio::process;
use tokio::process::Child;
use tonic::transport::Channel;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{ArtifactId, ArtifactSystem, ArtifactSystem::UnknownSystem},
        config::v0::config_service_client::ConfigServiceClient,
    },
};
use vorpal_store::paths::{get_private_key_path, setup_paths};
use vorpal_worker::service;

mod build;
mod log;
mod worker;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

async fn start_config(file: String) -> Result<(Child, ConfigServiceClient<Channel>)> {
    let port = random_free_port()
        .ok_or_else(|| anyhow!("{} failed to find free port", connector_end()))?;

    let mut command = process::Command::new(file);

    command.args(["start", "--port", &port.to_string()]);

    let mut process = command
        .spawn()
        .map_err(|_| anyhow!("failed to start config server"))?;

    // TODO: wait for output then proceed instead of sleeping

    tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;

    let host = format!("http://localhost:{:?}", port);

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

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(long, short)]
        file: String,

        #[arg(long, short)]
        artifact: String,

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

    #[clap(subcommand)]
    Worker(CommandWorker),
}

#[derive(Subcommand)]
pub enum CommandKeys {
    Generate {},
}

#[derive(Subcommand)]
pub enum CommandWorker {
    Start {
        #[clap(default_value_t = Level::INFO, global = true, long)]
        level: Level,

        #[clap(default_value = "23151", long, short)]
        port: u16,
    },
}

fn get_default_system() -> String {
    format!("{}-{}", ARCH, OS)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Build {
            file,
            artifact,
            system,
            worker,
        } => {
            if worker.is_empty() {
                bail!("{} no worker specified", connector_end());
            }

            let artifact_system: ArtifactSystem = get_artifact_system(system);

            if artifact_system == UnknownSystem {
                bail!(
                    "{} unknown target: {}",
                    connector_end(),
                    artifact_system.as_str_name()
                );
            }

            setup_paths().await?;

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                bail!(
                    "{} private key not found - run 'vorpal keys generate' or copy from worker",
                    connector_end()
                );
            }

            let (mut config_process, mut config_service) = start_config(file.to_string()).await?;

            let (artifacts_map, artifacts_order) =
                build::load_config(artifact, &mut config_service).await?;

            let mut artifacts_ids = HashMap::<String, ArtifactId>::new();

            // let artifacts_pending = artifacts_order.clone();

            for id in &artifacts_order {
                match artifacts_map.get(id) {
                    None => bail!("Build artifact not found: {}", id.name),
                    Some(artifact) => {
                        for a in &artifact.artifacts {
                            if artifacts_ids.get(&a.name).is_none() {
                                bail!("Artifact not found: {}", a.name);
                            }
                        }

                        build(artifact, id, artifact_system, worker).await?;

                        artifacts_ids.insert(id.name.clone(), id.clone());
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
                bail!(
                    "{} unknown target: {}",
                    connector_end(),
                    artifact_system.as_str_name()
                );
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

        Command::Worker(worker) => match worker {
            CommandWorker::Start { level, port } => {
                let mut subscriber = FmtSubscriber::builder().with_max_level(*level);

                if [Level::DEBUG, Level::TRACE].contains(level) {
                    subscriber = subscriber.with_file(true).with_line_number(true);
                }

                let subscriber = subscriber.finish();

                tracing::subscriber::set_global_default(subscriber)
                    .expect("setting default subscriber");

                service::listen(*port).await?;

                Ok(())
            }
        },
    }
}
