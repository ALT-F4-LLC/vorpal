use crate::worker::build;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{
    api::package::{
        PackageOutput, PackageSystem,
        PackageSystem::{Aarch64Linux, Aarch64Macos, Unknown},
    },
    get_package_system,
};
use vorpal_store::paths::{get_private_key_path, setup_paths};
use vorpal_worker::service;

mod config;
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

    #[clap(subcommand)]
    Keys(CommandKeys),

    Validate {
        #[arg(default_value = "vorpal.ncl", long, short)]
        file: String,

        #[arg(default_value_t = get_default_system(), long, short)]
        system: String,
    },

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
            let mut system: PackageSystem = get_package_system(system);

            if system == Unknown {
                anyhow::bail!("unknown target: {}", system.as_str_name());
            }

            if system == Aarch64Macos {
                system = Aarch64Linux;
            }

            if worker.is_empty() {
                anyhow::bail!("no worker specified");
            }

            setup_paths().await?;

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                anyhow::bail!(
                    "private key not found - run 'vorpal keys generate' or copy from worker"
                );
            }

            let (config, config_hash) = nickel::load_config(file, system).await?;

            let config_structures = config::build_structures(&config);

            let config_build_order = config::get_build_order(&config_structures.graph)?;

            let mut package_finished = HashMap::<String, PackageOutput>::new();

            for package_name in config_build_order {
                match config_structures.map.get(&package_name) {
                    None => anyhow::bail!("Package not found: {}", package_name),
                    Some(package) => {
                        let mut packages = vec![];

                        for p in &package.packages {
                            match package_finished.get(&p.name) {
                                None => eprintln!("Package not found: {}", p.name),
                                Some(package) => packages.push(package.clone()),
                            }
                        }

                        let output = build(&config_hash, package, packages, system, worker).await?;

                        package_finished.insert(package_name, output);
                    }
                }
            }

            Ok(())
        }

        Command::Keys(keys) => match keys {
            CommandKeys::Generate {} => {
                let key_dir_path = vorpal_store::paths::get_key_dir_path();
                let private_key_path = vorpal_store::paths::get_private_key_path();
                let public_key_path = vorpal_store::paths::get_public_key_path();

                if private_key_path.exists() {
                    println!("=> Private key exists: {}", private_key_path.display());
                    return Ok(())
                }

                if public_key_path.exists() {
                    println!("=> Public key exists: {}", public_key_path.display());
                    return Ok(())
                }

                vorpal_notary::generate_keys(key_dir_path, private_key_path, public_key_path)
                    .await?;

                Ok(())
            }
        },

        Command::Validate { file, system } => {
            println!("=> Validating: {}", file);

            let mut package_system: PackageSystem = get_package_system(system);

            if package_system == Unknown {
                anyhow::bail!("unknown target: {}", system);
            }

            if package_system == Aarch64Macos {
                package_system = Aarch64Linux;
            }

            let (config, _) = nickel::load_config(file, package_system).await?;

            println!("{}", serde_json::to_string_pretty(&config)?);

            Ok(())
        }

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
