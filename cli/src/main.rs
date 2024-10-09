use crate::{
    log::{connector_end, print_build_order},
    worker::build,
};
use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{
    get_package_system,
    vorpal::package::v0::{PackageOutput, PackageSystem, PackageSystem::UnknownSystem},
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

fn get_default_package() -> String {
    "default".to_string()
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(default_value = "vorpal.ncl", long, short)]
        file: String,

        #[arg(default_value_t = get_default_package(), long, short)]
        package: String,

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

            let config = config::check_config(file, Some(package), system).await?;

            let (build_map, build_order) = nickel::load_config_build(&config.packages)?;

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

        Command::Check { file, system } => {
            let _ = config::check_config(file, None, system).await?;

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
