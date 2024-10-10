use crate::{
    log::{connector_end, print_build_order},
    worker::build,
};
use anyhow::{anyhow, bail, Result};
use clap::{Parser, Subcommand};
use port_selector::random_free_port;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use tokio::process;
use tokio::process::Child;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{
    get_package_system,
    vorpal::config::v0::{config_service_client::ConfigServiceClient, Config, EvaluateRequest},
    vorpal::package::v0::{PackageOutput, PackageSystem, PackageSystem::UnknownSystem},
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

fn get_default_package() -> String {
    "default".to_string()
}

async fn shutdown_config(mut process: Child) -> Result<()> {
    process
        .kill()
        .await
        .map_err(|_| anyhow!("{} failed to kill config server", connector_end()))?;

    Ok(())
}

async fn get_config(file: String, system: PackageSystem) -> Result<Config> {
    let config_port = random_free_port()
        .ok_or_else(|| anyhow!("{} failed to find free port", connector_end()))?;

    let mut config_command = process::Command::new(file);

    config_command.args(&["start", "--port", &config_port.to_string()]);

    let config_process = config_command
        .spawn()
        .map_err(|_| anyhow!("{} failed to start config server", connector_end()))?;

    // TODO: wait for output then proceed instead of sleeping

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let config_host = format!("http://localhost:{}", config_port);

    let mut config_service = match ConfigServiceClient::connect(config_host).await {
        Ok(config_service) => config_service,
        Err(e) => {
            shutdown_config(config_process).await?;
            bail!(
                "{} failed to connect to config server: {}",
                connector_end(),
                e
            );
        }
    };

    let config_response = match config_service
        .evaluate(EvaluateRequest {
            system: system.into(),
        })
        .await
    {
        Ok(response) => response,
        Err(error) => {
            shutdown_config(config_process).await?;
            bail!("{} failed to evaluate config: {}", connector_end(), error);
        }
    };

    shutdown_config(config_process).await?;

    Ok(config_response.into_inner().config.unwrap())
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(long, short)]
        file: String,

        #[arg(default_value_t = get_default_package(), long, short)]
        package: String,

        #[arg(default_value_t = get_default_system(), long, short)]
        system: String,

        #[clap(default_value = "http://localhost:23151", long, short)]
        worker: String,
    },

    Config {
        #[arg(long, short)]
        file: String,

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

            let config = get_config(file.to_string(), package_system).await?;

            if !config.packages.contains_key(package) {
                bail!("package not found: {}", package);
            }

            let (build_map, build_order) = build::load_config(&config.packages)?;

            log::print_packages(&build_order);

            print_build_order(&build_order);

            let mut package_output = HashMap::<String, PackageOutput>::new();

            for package_name in &build_order {
                match build_map.get(package_name) {
                    None => bail!("Package not found: {}", package_name),
                    Some(package) => {
                        let mut packages = vec![];

                        for p in &package.packages {
                            match package_output.get(&p.name) {
                                None => bail!("Package not found: {}", p.name),
                                Some(package) => packages.push(package.clone()),
                            }
                        }

                        let output = build(package, packages, package_system, worker).await?;

                        package_output.insert(package_name.to_string(), output);
                    }
                }
            }

            Ok(())
        }

        Command::Config { file, system } => {
            let package_system: PackageSystem = get_package_system(system);

            if package_system == UnknownSystem {
                bail!(
                    "{} unknown target: {}",
                    connector_end(),
                    package_system.as_str_name()
                );
            }

            let config = get_config(file.to_string(), package_system).await?;

            let config_json = serde_json::to_string_pretty(&config).map_err(|e| {
                anyhow!(
                    "{} failed to serialize config to json: {}",
                    connector_end(),
                    e
                )
            })?;

            println!("{}", config_json);

            Ok(())
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

                service::start(*port).await?;

                Ok(())
            }
        },
    }
}
