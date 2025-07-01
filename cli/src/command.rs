use anyhow::Result;
use clap::{Parser, Subcommand};
use path_clean::PathClean;
use serde::Deserialize;
use std::{
    env::{current_dir, var},
    path::PathBuf,
    process::exit,
};
use tokio::fs::read;
use toml::from_str;
use tracing::{error, subscriber, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};
use vorpal_sdk::artifact::system::get_system_default_str;

mod artifact;
mod init;
mod start;
mod store;
mod system;

fn get_default_address() -> String {
    "http://localhost:23151".to_string()
}

async fn load_vorpal_toml(path: PathBuf) -> Option<VorpalToml> {
    if !path.exists() {
        return None;
    }

    match read(&path).await {
        Ok(toml_data_bytes) => {
            let toml_data_str = String::from_utf8_lossy(&toml_data_bytes);
            match from_str::<VorpalToml>(&toml_data_str) {
                Ok(toml) => Some(toml),
                Err(e) => {
                    error!("Failed to parse config at {}: {}", path.display(), e);
                    None
                }
            }
        }
        Err(e) => {
            error!("Failed to read config at {}: {}", path.display(), e);
            None
        }
    }
}

fn get_home_config_path() -> Option<PathBuf> {
    var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".vorpal").join("Vorpal.toml"))
}

fn merge_configs(
    home_config: Option<VorpalToml>,
    project_config: Option<VorpalToml>,
) -> Option<VorpalToml> {
    match (home_config, project_config) {
        (None, None) => None,
        (Some(home), None) => Some(home),
        (None, Some(project)) => Some(project),
        (Some(home), Some(project)) => {
            // Project config takes precedence over home config
            let merged_config = match (home.config, project.config) {
                (None, None) => None,
                (Some(home_cfg), None) => Some(home_cfg),
                (None, Some(project_cfg)) => Some(project_cfg),
                (Some(home_cfg), Some(project_cfg)) => Some(VorpalTomlConfig {
                    language: project_cfg.language.or(home_cfg.language),
                    name: project_cfg.name.or(home_cfg.name),
                    source: merge_config_sources(home_cfg.source, project_cfg.source),
                }),
            };

            Some(VorpalToml {
                config: merged_config,
                registry: project.registry.or(home.registry),
            })
        }
    }
}

fn merge_config_sources(
    home_source: Option<VorpalTomlConfigSource>,
    project_source: Option<VorpalTomlConfigSource>,
) -> Option<VorpalTomlConfigSource> {
    match (home_source, project_source) {
        (None, None) => None,
        (Some(home), None) => Some(home),
        (None, Some(project)) => Some(project),
        (Some(home), Some(project)) => Some(VorpalTomlConfigSource {
            go: merge_go_configs(home.go, project.go),
            includes: merge_includes(home.includes, project.includes),
            rust: merge_rust_configs(home.rust, project.rust),
        }),
    }
}

fn merge_go_configs(
    home_go: Option<VorpalTomlConfigSourceGo>,
    project_go: Option<VorpalTomlConfigSourceGo>,
) -> Option<VorpalTomlConfigSourceGo> {
    match (home_go, project_go) {
        (None, None) => None,
        (Some(home), None) => Some(home),
        (None, Some(project)) => Some(project),
        (Some(home), Some(project)) => Some(VorpalTomlConfigSourceGo {
            directory: project.directory.or(home.directory),
        }),
    }
}

fn merge_rust_configs(
    home_rust: Option<VorpalConfigSourceRust>,
    project_rust: Option<VorpalConfigSourceRust>,
) -> Option<VorpalConfigSourceRust> {
    match (home_rust, project_rust) {
        (None, None) => None,
        (Some(home), None) => Some(home),
        (None, Some(project)) => Some(project),
        (Some(home), Some(project)) => Some(VorpalConfigSourceRust {
            bin: project.bin.or(home.bin),
            packages: merge_packages(home.packages, project.packages),
        }),
    }
}

fn merge_includes(
    home_includes: Option<Vec<String>>,
    project_includes: Option<Vec<String>>,
) -> Option<Vec<String>> {
    match (home_includes, project_includes) {
        (None, None) => None,
        (Some(home), None) => Some(home),
        (None, Some(project)) => Some(project),
        (Some(mut home), Some(project)) => {
            // Combine includes, with project includes taking precedence for duplicates
            for include in project {
                if !home.contains(&include) {
                    home.push(include);
                }
            }
            Some(home)
        }
    }
}

fn merge_packages(
    home_packages: Option<Vec<String>>,
    project_packages: Option<Vec<String>>,
) -> Option<Vec<String>> {
    match (home_packages, project_packages) {
        (None, None) => None,
        (Some(home), None) => Some(home),
        (None, Some(project)) => Some(project),
        (Some(mut home), Some(project)) => {
            // Combine packages, with project packages taking precedence for duplicates
            for package in project {
                if !home.contains(&package) {
                    home.push(package);
                }
            }
            Some(home)
        }
    }
}

#[derive(Subcommand)]
pub enum CommandArtifact {
    Init {},

    Inspect {
        /// Artifact digest to inspect
        digest: String,
    },

    Make {
        /// Artifact name
        name: String,

        /// Artifact context
        context: PathBuf,

        /// Agent address
        #[arg(default_value_t = get_default_address(), long)]
        agent: String,

        #[arg(long)]
        alias: Vec<String>,

        #[arg(default_value = "Vorpal.toml", long)]
        config: PathBuf,

        #[arg(default_value_t = false, long)]
        export: bool,

        #[arg(default_value_t = false, long)]
        lockfile_update: bool,

        #[arg(default_value_t = false, long)]
        path: bool,

        #[arg(default_value_t = false, long)]
        rebuild: bool,

        #[arg(default_value_t = get_system_default_str(), long)]
        system: String,

        #[arg(long)]
        variable: Vec<String>,

        /// Worker address
        #[arg(default_value_t = get_default_address(), long)]
        worker: String,
    },
}

#[derive(Subcommand)]
pub enum CommandSystemKeys {
    Generate {},
}

#[derive(Subcommand)]
pub enum CommandServices {
    Start {
        #[arg(default_value = "23151", long)]
        port: u16,

        #[arg(default_value = "agent,registry,worker", long)]
        services: String,

        #[arg(default_value = "local", long)]
        registry_backend: String,

        #[arg(long)]
        registry_backend_s3_bucket: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum CommandSystem {
    #[clap(subcommand)]
    Keys(CommandSystemKeys),

    Prune {
        #[arg(default_value_t = false, long)]
        all: bool,

        #[arg(long)]
        aliases: bool,

        #[arg(long)]
        archives: bool,

        #[arg(long)]
        configs: bool,

        #[arg(long)]
        outputs: bool,
    },
}

#[derive(Subcommand)]
pub enum Command {
    #[clap(subcommand)]
    Artifact(CommandArtifact),

    #[clap(subcommand)]
    Services(CommandServices),

    #[clap(subcommand)]
    System(CommandSystem),
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    // Log level
    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,

    /// Registry address
    #[arg(long)]
    registry: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VorpalTomlConfigSourceGo {
    pub directory: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct VorpalConfigSourceRust {
    bin: Option<String>,
    packages: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
struct VorpalTomlConfigSource {
    go: Option<VorpalTomlConfigSourceGo>,
    includes: Option<Vec<String>>,
    rust: Option<VorpalConfigSourceRust>,
}

#[derive(Clone, Debug, Deserialize)]
struct VorpalTomlConfig {
    language: Option<String>,
    name: Option<String>,
    source: Option<VorpalTomlConfigSource>,
}

#[derive(Clone, Debug, Deserialize)]
struct VorpalToml {
    config: Option<VorpalTomlConfig>,
    registry: Option<String>,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    let Cli {
        command,
        level,
        registry: cli_registry,
    } = cli;

    // Set up tracing subscriber

    let subscriber_writer = std::io::stderr.with_max_level(level);

    let mut subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_writer(subscriber_writer)
        .without_time();

    if [Level::DEBUG, Level::TRACE].contains(&level) {
        subscriber = subscriber.with_file(true).with_line_number(true);
    }

    let subscriber = subscriber.finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber");

    match &command {
        Command::Artifact(artifact) => match artifact {
            CommandArtifact::Init {} => init::run().await,

            CommandArtifact::Inspect { digest } => {
                let mut registry = get_default_address();

                if let Some(home_path) = get_home_config_path() {
                    if let Some(home_config) = load_vorpal_toml(home_path).await {
                        if let Some(home_registry) = home_config.registry {
                            registry = home_registry;
                        }
                    }
                }

                if let Some(r) = cli_registry {
                    registry = r;
                }

                artifact::inspect::run(digest, &registry).await
            }

            CommandArtifact::Make {
                agent,
                alias,
                config,
                context,
                export,
                lockfile_update,
                name,
                path,
                rebuild,
                system,
                variable,
                worker,
            } => {
                if name.is_empty() {
                    error!("no name specified");
                    exit(1);
                }

                // Set default configuration
                let mut config_language = "rust".to_string();
                let mut config_name = "vorpal".to_string();
                let mut config_source_go_directory = None;
                let mut config_source_includes = Vec::new();
                let mut config_source_rust_bin = None;
                let mut config_source_rust_packages = None;

                // Load configuration values from home and project
                let home_config = if let Some(home_path) = get_home_config_path() {
                    load_vorpal_toml(home_path).await
                } else {
                    None
                };

                let config_absolute_path = match config.is_absolute() {
                    true => config.to_path_buf(),
                    false => {
                        let current_dir = current_dir().expect("failed to get current directory");
                        current_dir.join(config).clean().to_path_buf()
                    }
                }
                .clean();

                let project_config = load_vorpal_toml(config_absolute_path).await;

                // Determine final registry value
                let mut registry = get_default_address();

                if let Some(ref hc) = home_config {
                    if let Some(ref r) = hc.registry {
                        registry = r.clone();
                    }
                }

                if let Some(ref pc) = project_config {
                    if let Some(ref r) = pc.registry {
                        registry = r.clone();
                    }
                }

                if let Some(ref r) = cli_registry {
                    registry = r.clone();
                }

                // Merge configurations (project overrides home)
                if let Some(merged_toml) = merge_configs(home_config, project_config) {
                    if let Some(config) = merged_toml.config {
                        if let Some(language) = config.language {
                            config_language = language;
                        }

                        if let Some(name) = config.name {
                            config_name = name;
                        }

                        if let Some(source) = config.source {
                            if let Some(go) = source.go {
                                if let Some(directory) = go.directory {
                                    config_source_go_directory = Some(directory);
                                }
                            }

                            if let Some(includes) = source.includes {
                                config_source_includes = includes;
                            }

                            if let Some(rust) = source.rust {
                                if let Some(bin) = rust.bin {
                                    config_source_rust_bin = Some(bin);
                                }

                                if let Some(packages) = rust.packages {
                                    config_source_rust_packages = Some(packages);
                                }
                            }
                        }
                    }
                }

                let context_absolute_path = match context.is_absolute() {
                    true => context.to_path_buf(),
                    false => {
                        let current_dir = current_dir().expect("failed to get current directory");
                        current_dir.join(context).clean().to_path_buf()
                    }
                }
                .clean();

                let artifact_args = artifact::make::RunArgsArtifact {
                    aliases: alias.clone(),
                    context: context_absolute_path.clone(),
                    export: *export,
                    lockfile_update: *lockfile_update,
                    name: name.clone(),
                    path: *path,
                    rebuild: *rebuild,
                    system: system.clone(),
                    variable: variable.clone(),
                };

                let config_args = artifact::make::RunArgsConfig {
                    context: context_absolute_path,
                    language: config_language,
                    name: config_name,
                    source: Some(VorpalTomlConfigSource {
                        go: Some(VorpalTomlConfigSourceGo {
                            directory: config_source_go_directory,
                        }),
                        includes: Some(config_source_includes),
                        rust: Some(VorpalConfigSourceRust {
                            bin: config_source_rust_bin,
                            packages: config_source_rust_packages,
                        }),
                    }),
                };

                let service_args = artifact::make::RunArgsService {
                    agent: agent.to_string(),
                    registry,
                    worker: worker.to_string(),
                };

                artifact::make::run(artifact_args, config_args, service_args).await
            }
        },

        Command::Services(services) => match services {
            CommandServices::Start {
                port,
                registry_backend,
                registry_backend_s3_bucket,
                services,
            } => {
                let mut registry = get_default_address();

                if let Some(home_path) = get_home_config_path() {
                    if let Some(home_config) = load_vorpal_toml(home_path).await {
                        if let Some(home_registry) = home_config.registry {
                            registry = home_registry;
                        }
                    }
                }

                if let Some(r) = cli_registry {
                    registry = r;
                }

                start::run(
                    *port,
                    registry,
                    registry_backend.clone(),
                    registry_backend_s3_bucket.clone(),
                    services.split(',').map(|s| s.to_string()).collect(),
                )
                .await
            }
        },

        Command::System(system) => match system {
            CommandSystem::Keys(keys) => match keys {
                CommandSystemKeys::Generate {} => system::keys::generate().await,
            },

            CommandSystem::Prune {
                aliases,
                all,
                archives,
                configs,
                outputs,
            } => system::prune::run(*aliases, *all, *archives, *configs, *outputs).await,
        },
    }
}
