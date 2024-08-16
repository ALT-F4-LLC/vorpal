use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::Path;
use vorpal_schema::{api::package::PackageOutput, get_package_target};
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

#[derive(Subcommand)]
enum Command {
    Build {
        #[clap(short, long, default_value = "vorpal.ncl")]
        file: String,

        #[clap(short, long)]
        workers: Vec<String>,
    },

    #[clap(subcommand)]
    Keys(Keys),
}

#[derive(Subcommand)]
pub enum Keys {
    Generate {},
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Build { file, workers } => {
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

            // let default_target = get_package_target(format!("{}-{}", ARCH, OS).as_str());

            // if !workers.iter().any(|w| w.system == default_target) {
            //     warn!("no workers for current system");
            // }

            // Create directories

            setup_paths().await?;

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                return Err(anyhow::anyhow!(
                    "private key not found - run 'vorpal keys generate' or copy from worker"
                ));
            }

            // Load configuration

            let config_path = Path::new(file).to_path_buf();

            let (config, config_hash) = nickel::load_config(config_path)?;

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
                        let mut packages = vec![];

                        for p in &package.packages {
                            match package_finished.get(&p.name) {
                                None => eprintln!("Package not found: {}", p.name),
                                Some(package) => packages.push(package.clone()),
                            }
                        }

                        let output =
                            worker::build(&config_hash, package, packages, &workers).await?;

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
    }
}
