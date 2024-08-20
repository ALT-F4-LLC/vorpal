use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use vorpal_schema::{
    api::package::{PackageOutput, PackageSystem},
    get_package_target,
};
use vorpal_store::paths::{get_private_key_path, setup_paths};

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

fn get_default_target() -> String {
    format!("{}-{}", ARCH, OS)
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(short, long, default_value = "vorpal.ncl")]
        file: String,

        #[arg(short, long, default_value_t = get_default_target())]
        target: String,

        #[arg(short, long)]
        workers: Vec<String>,
    },

    #[clap(subcommand)]
    Keys(Keys),

    Validate {
        #[arg(short, long, default_value = "vorpal.ncl")]
        file: String,

        #[arg(short, long, default_value_t = get_default_target())]
        target: String,
    },
}

#[derive(Subcommand)]
pub enum Keys {
    Generate {},
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Build {
            file,
            target,
            workers,
        } => {
            // Parse target

            let mut target: PackageSystem = get_package_target(&target);

            if target == PackageSystem::Unknown {
                eprintln!("unknown target: {}", target.as_str_name());
                return Ok(());
            }

            if target == PackageSystem::Aarch64Macos {
                target = PackageSystem::Aarch64Linux;
            }

            // Parse workers

            let workers: Vec<worker::Worker> = workers
                .iter()
                .map(|worker| {
                    let parts: Vec<&str> = worker.split('=').collect();
                    worker::Worker {
                        system: get_package_target(parts[0]),
                        uri: parts[1].to_string(),
                    }
                })
                .collect();

            if workers.is_empty() {
                eprintln!("no workers specified");
                return Ok(());
            }

            // Create directories

            setup_paths().await?;

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                return Err(anyhow::anyhow!(
                    "private key not found - run 'vorpal keys generate' or copy from worker"
                ));
            }

            // Load configuration

            let (config, config_hash) = nickel::load_config(file, target)?;

            // Get target

            if !workers.iter().any(|w| w.system == target) {
                println!(
                    "no worker specified for target '{}', using default '{}'",
                    target.as_str_name(),
                    workers[0].uri
                );
            }

            // Generate build order

            let config_structures = config::build_structures(&config);

            let config_build_order = config::get_build_order(&config_structures.graph)?;

            // Build packages

            // TODO: run builds in parallel
            let mut package_finished = HashMap::<String, PackageOutput>::new();

            for package_name in config_build_order {
                match config_structures.map.get(&package_name) {
                    None => eprintln!("Package not found: {}", package_name),
                    Some(package) => {
                        let mut packages_built = vec![];

                        for p in &package.packages {
                            match package_finished.get(&p.name) {
                                None => eprintln!("Package not found: {}", p.name),
                                Some(package) => packages_built.push(package.clone()),
                            }
                        }

                        let output =
                            worker::build(&config_hash, package, packages_built, target, &workers)
                                .await?;

                        package_finished.insert(package_name, output);
                    }
                }
            }

            Ok(())
        }

        Command::Keys(keys) => match keys {
            Keys::Generate {} => {
                let key_dir_path = vorpal_store::paths::get_key_dir_path();

                let private_key_path = vorpal_store::paths::get_private_key_path();

                let public_key_path = vorpal_store::paths::get_public_key_path();

                vorpal_notary::generate_keys(key_dir_path, private_key_path, public_key_path)
                    .await?;

                Ok(())
            }
        },

        Command::Validate { file, target } => {
            let mut target: PackageSystem = get_package_target(&target);

            if target == PackageSystem::Unknown {
                eprintln!("unknown target: {}", target.as_str_name());
                return Ok(());
            }

            if target == PackageSystem::Aarch64Macos {
                target = PackageSystem::Aarch64Linux;
            }

            let (config, _) = nickel::load_config(file, target)?;

            println!("{}", serde_json::to_string_pretty(&config)?);

            Ok(())
        }
    }
}
