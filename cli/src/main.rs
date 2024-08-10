use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env::consts::{ARCH, OS};
use std::path::Path;
use tracing::Level;
use tracing::{error, info, warn};
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{api::package::PackageSystem, get_package_target};
use vorpal_store::paths::{get_private_key_path, setup_paths};

mod config;
mod nickel;
mod worker;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[clap(long, global = true, default_value_t = Level::INFO)]
    level: tracing::Level,

    #[command(subcommand)]
    command: Command,
}

pub fn get_default_workers() -> String {
    let target: PackageSystem = get_package_target(format!("{}-{}", ARCH, OS).as_str());
    let target_dashes = target.as_str_name().to_lowercase().replace("_", "-");
    format!("{}=http://localhost:23151", target_dashes)
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[clap(short, long, default_value = "vorpal.ncl")]
        file: String,

        #[clap(short, long, default_value = get_default_workers())]
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

    let mut subscriber = FmtSubscriber::builder().with_max_level(cli.level);

    // when we run the command with `TRACE` or `DEBUG` level, we want to see
    // the file and line number...
    if [Level::DEBUG, Level::TRACE].contains(&cli.level) {
        subscriber = subscriber.with_file(true).with_line_number(true);
    }

    let subscriber = subscriber.finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber");

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
                error!("no workers specified");
                return Ok(());
            }

            let default_target = get_package_target(format!("{}-{}", ARCH, OS).as_str());

            if !workers.iter().any(|w| w.system == default_target) {
                warn!("no workers for current system");
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

            let config_path = Path::new(file).to_path_buf();

            let (config, config_hash) = nickel::load_config(config_path)?;

            info!("Config hash: {:?}", config_hash);

            // Generate build order

            let config_structures = config::build_structures(&config);

            let config_build_order = config::get_build_order(&config_structures.graph)?;

            // Build packages

            // TODO: run builds in parallel

            for package_name in config_build_order {
                match config_structures.map.get(&package_name) {
                    None => error!("Package not found: {}", package_name),
                    Some(package) => {
                        worker::build(package, &config_hash, default_target, &workers).await?;
                    }
                }
            }

            Ok(())
        }

        Command::Keys(keys) => match keys {
            Keys::Generate {} => {
                let key_path = vorpal_store::paths::get_key_path();
                let private_key_path = vorpal_store::paths::get_private_key_path();
                let public_key_path = vorpal_store::paths::get_public_key_path();
                vorpal_notary::generate_keys(key_path, private_key_path, public_key_path).await?;
                Ok(())
            }
        },
    }
}
