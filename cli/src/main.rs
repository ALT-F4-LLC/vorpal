use anyhow::Result;
use clap::{Parser, Subcommand};
use nickel_lang_core::eval::cache::lazy::CBNCache;
use nickel_lang_core::program::Program;
use nickel_lang_core::serialize;
use nickel_lang_core::serialize::ExportFormat::Json;
use std::env::consts::{ARCH, OS};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{api::package::PackageSystem, get_package_target, Config};

mod config;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[clap(long, global = true, default_value_t = Level::INFO)]
    level: tracing::Level,

    #[command(subcommand)]
    command: Command,
}

fn get_default_workers() -> String {
    let target: PackageSystem = get_package_target(format!("{}-{}", ARCH, OS).as_str());
    let target_dashes = target.as_str_name().to_lowercase().replace("_", "-");
    format!("{}=http://localhost:23151", target_dashes)
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[clap(short, long, default_value = get_default_workers())]
        workers: String,
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
        Command::Build { workers } => {
            println!("Building with workers: {}", workers);

            let config_path = std::path::Path::new("vorpal.ncl");

            let mut program = Program::<CBNCache>::new_from_file(config_path, std::io::stderr())?;

            if let Ok(nickel_path) = std::env::var("NICKEL_IMPORT_PATH") {
                program.add_import_paths(nickel_path.split(':'));
            }

            let eval = match program.eval_full_for_export() {
                Ok(eval) => eval,
                Err(err) => {
                    eprintln!("{:?}", err);
                    std::process::exit(1);
                }
            };

            match serialize::validate(Json, &eval) {
                Ok(_) => {}
                Err(err) => {
                    eprintln!("{:?}", err);
                    std::process::exit(1);
                }
            }

            let data = match serialize::to_string(Json, &eval) {
                Ok(data) => data,
                Err(err) => {
                    eprintln!("{:?}", err);
                    std::process::exit(1);
                }
            };

            let config: Config = serde_json::from_str(&data)?;
            let config_structures = config::build_structures(&config);
            let config_build_order = config::get_build_order(&config_structures.graph)?;

            for package_name in config_build_order {
                match config_structures.map.get(&package_name) {
                    None => eprintln!("Package not found: {}", package_name),
                    Some(package) => {
                        println!("Building package: {}", package.name)

                        // TODO: build package
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
