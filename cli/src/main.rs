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
    get_package_system,
    vorpal::{
        config::v0::config_service_client::ConfigServiceClient,
        package::v0::{PackageOutput, PackageSystem, PackageSystem::UnknownSystem},
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
        package: String,

        #[arg(default_value_t = get_default_system(), long, short)]
        system: String,

        #[clap(default_value = "http://localhost:23151", long, short)]
        worker: String,
    },

    Config {
        #[arg(long, short)]
        file: String,

        #[arg(long, short)]
        package: String,

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
            package,
            system,
            worker,
        } => {
            if worker.is_empty() {
                bail!("{} no worker specified", connector_end());
            }

            let package_system: PackageSystem = get_package_system(system);

            if package_system == UnknownSystem {
                bail!(
                    "{} unknown target: {}",
                    connector_end(),
                    package_system.as_str_name()
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

            let (packages_map, packages_order) =
                build::load_config(package, &mut config_service).await?;

            let mut package_output = HashMap::<String, PackageOutput>::new();

            for package in &packages_order {
                match packages_map.get(package) {
                    None => bail!("Build package not found: {}", package.name),
                    Some(package) => {
                        let mut packages = vec![];

                        for p in &package.packages {
                            match package_output.get(&p.name) {
                                None => bail!("Package output not found: {}", p.name),
                                Some(package) => packages.push(package.clone()),
                            }
                        }

                        let output = build(package, packages, package_system, worker).await?;

                        package_output.insert(package.name.to_string(), output);
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
            package,
            system,
        } => {
            let package_system: PackageSystem = get_package_system(system);

            if package_system == UnknownSystem {
                bail!(
                    "{} unknown target: {}",
                    connector_end(),
                    package_system.as_str_name()
                );
            }

            let (mut config_process, mut config_service) = start_config(file.to_string()).await?;

            let _ = build::load_config(package, &mut config_service).await?;

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
