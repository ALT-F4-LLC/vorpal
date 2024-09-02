use crate::log::{connector_end, print_build_order};
use crate::worker::build;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{
    api::package::{PackageOutput, PackageSystem, PackageSystem::Unknown},
    get_package_system,
};
use vorpal_store::paths::{get_private_key_path, setup_paths};
use vorpal_worker::service;

mod config;
mod log;
mod nickel;
mod worker;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(default_value = "vorpal.ncl", long, short)]
        file: String,

        #[arg(default_value_t = get_default_system(), long, short)]
        system: String,

        #[clap(default_value = "http://localhost:23151", long, short)]
        worker: String,
    },

    Check {
        #[arg(default_value = "vorpal.ncl", long, short)]
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
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Build {
            file,
            system,
            worker,
        } => {
            if worker.is_empty() {
                anyhow::bail!("{} no worker specified", connector_end());
            }

            let package_system: PackageSystem = get_package_system(system);

            if package_system == Unknown {
                anyhow::bail!(
                    "{} unknown target: {}",
                    connector_end(),
                    package_system.as_str_name()
                );
            }

            setup_paths().await?;

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                anyhow::bail!(
                    "{} private key not found - run 'vorpal keys generate' or copy from worker",
                    connector_end()
                );
            }

            let (config_map, config_order, config_hash) =
                config::check_config(file, system).await?;

            print_build_order(&config_order);

            let mut package_output = HashMap::<String, PackageOutput>::new();

            for package_name in config_order {
                match config_map.get(&package_name) {
                    None => anyhow::bail!("Package not found: {}", package_name),
                    Some(package) => {
                        let mut packages = vec![];

                        for p in &package.packages {
                            match package_output.get(&p.name) {
                                None => eprintln!("Package not found: {}", p.name),
                                Some(package) => packages.push(package.clone()),
                            }
                        }

                        let output =
                            build(&config_hash, package, packages, package_system, worker).await?;

                        package_output.insert(package_name.to_string(), output);
                    }
                }
            }

            Ok(())
        }

        Command::Check { file, system } => {
            let _ = config::check_config(file, system).await?;

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
                    anyhow::bail!("private key exists but public key is missing");
                }

                if !private_key_path.exists() && public_key_path.exists() {
                    anyhow::bail!("public key exists but private key is missing");
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
