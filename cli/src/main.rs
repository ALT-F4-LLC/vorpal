use anyhow::Result;
use clap::{Parser, Subcommand};
use nickel_lang_core::eval::cache::lazy::CBNCache;
use nickel_lang_core::program::Program;
use nickel_lang_core::serialize;
use nickel_lang_core::serialize::ExportFormat::Json;
use std::collections::{HashMap, HashSet};
use std::env::consts::{ARCH, OS};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use vorpal_schema::{api::package::PackageSystem, get_package_target, Config, Package};

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

fn topological_sort(
    package_graph: &HashMap<String, HashSet<String>>,
    build_order: &mut Vec<String>,
    visited: &mut HashSet<String>,
    stack: &mut HashSet<String>,
    package: &str,
) -> Result<(), String> {
    if stack.contains(package) {
        return Err(format!("Circular dependency detected: {}", package));
    }

    if visited.contains(package) {
        return Ok(());
    }

    stack.insert(package.to_string());

    if let Some(deps) = package_graph.get(package) {
        for dep in deps {
            topological_sort(package_graph, build_order, visited, stack, dep)?;
        }
    }

    stack.remove(package);

    visited.insert(package.to_string());

    build_order.push(package.to_string());

    Ok(())
}

fn add_to_map(package_map: &mut HashMap<String, Package>, package: &Package) {
    package_map.insert(package.name.clone(), package.clone());

    for dep in &package.packages {
        add_to_map(package_map, dep);
    }
}

fn add_to_graph(package_graph: &mut HashMap<String, HashSet<String>>, package: &Package) {
    let dependencies: HashSet<String> = package.packages.iter().map(|p| p.name.clone()).collect();

    package_graph.insert(package.name.clone(), dependencies);

    for dep in &package.packages {
        add_to_graph(package_graph, dep);
    }
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

            let mut package_graph: HashMap<String, HashSet<String>> = HashMap::new();
            let mut package_map = HashMap::new();

            for (_, package) in &config.packages {
                add_to_graph(&mut package_graph, package);
                add_to_map(&mut package_map, package);
            }

            let mut build_order = Vec::new();
            let mut stack = HashSet::new();
            let mut visited = HashSet::new();

            for package in package_graph.keys() {
                if !visited.contains(package) {
                    topological_sort(
                        &package_graph,
                        &mut build_order,
                        &mut visited,
                        &mut stack,
                        package,
                    )
                    .expect("Failed to sort packages");
                }
            }

            for package_name in build_order {
                match package_map.get(&package_name) {
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
