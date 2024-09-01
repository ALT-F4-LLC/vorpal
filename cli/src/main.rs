use crate::worker::build;
use anyhow::Result;
use clap::{Parser, Subcommand};
use console::style;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::path::Path;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{
    api::package::{PackageOutput, PackageSystem, PackageSystem::Unknown},
    get_package_system, Package,
};
use vorpal_store::paths::{
    get_package_archive_path, get_package_path, get_private_key_path, setup_paths,
};
use vorpal_worker::service;

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

fn render_package(
    package_name: &str,
    build_map: &HashMap<&str, Package>,
    prefix: &str,
    is_last: bool,
) {
    match build_map.get(package_name) {
        None => eprintln!("Package not found: {}", package_name),
        Some(package) => {
            let hash_default = "none".to_string();

            let hash = package.source_hash.as_ref().unwrap_or(&hash_default);

            let connector = if is_last { "└── " } else { "├── " };

            let exists = get_package_path(hash, package_name).exists();

            let exists_archive = get_package_archive_path(hash, package_name).exists();

            let cached = if exists || exists_archive {
                style("[✓]").green()
            } else {
                style("[✗]").red()
            };

            let name = style(package_name).bold();

            let connector = style(connector).dim();

            let prefix = style(prefix).dim();

            println!("{}{}{} {}", prefix, connector, name, cached);

            let new_prefix = if is_last { "    " } else { "│   " };

            let new_prefix = style(new_prefix).dim();

            for (i, p) in package.packages.iter().enumerate() {
                let is_last = i == package.packages.len() - 1;

                render_package(
                    p.name.as_str(),
                    build_map,
                    &format!("{}{}", prefix, new_prefix),
                    is_last,
                );
            }
        }
    }
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
                anyhow::bail!("no worker specified");
            }

            let package_system: PackageSystem = get_package_system(system);

            if package_system == Unknown {
                anyhow::bail!("unknown target: {}", package_system.as_str_name());
            }

            setup_paths().await?;

            let private_key_path = get_private_key_path();

            if !private_key_path.exists() {
                anyhow::bail!(
                    "private key not found - run 'vorpal keys generate' or copy from worker"
                );
            }

            let file_path = Path::new(file);

            if !file_path.exists() {
                anyhow::bail!("config not found: {}", file_path.display());
            }

            let (config, config_hash) = nickel::load_config(file_path, package_system).await?;

            let (_, build_map, build_order) = nickel::load_config_build(&config.packages)?;

            println!("=> Packages:");

            for (index, package_name) in build_order.iter().enumerate() {
                let is_last = index == build_order.len() - 1;
                render_package(package_name, &build_map, "", is_last);
            }

            println!("=> Building: {} ({})", file, system);

            let mut package_output = HashMap::<String, PackageOutput>::new();

            for package_name in build_order {
                match build_map.get(&package_name) {
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
            let package_system: PackageSystem = get_package_system(system);

            if package_system == Unknown {
                anyhow::bail!("unknown target: {}", system);
            }

            let file_path = Path::new(file);

            if !file_path.exists() {
                anyhow::bail!("config not found: {}", file_path.display());
            }

            println!("=> Config: {} ({})", file_path.display(), system);

            let (config, _) = nickel::load_config(file_path, package_system).await?;

            println!("=> Packages:");

            let (_, build_map, build_order) = nickel::load_config_build(&config.packages)?;

            for (index, package_name) in build_order.iter().enumerate() {
                let is_last = index == build_order.len() - 1;
                render_package(package_name, &build_map, "", is_last);
            }

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
