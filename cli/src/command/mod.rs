use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env::current_dir;
use tracing::Level;
use vorpal_sdk::system::get_system_default_str;

mod artifact;
mod init;
mod keys;
mod start;
mod store;

fn default_artifact_context() -> String {
    current_dir()
        .expect("failed to get current directory")
        .to_string_lossy()
        .to_string()
}

#[derive(Subcommand)]
pub enum Command {
    Artifact {
        #[clap(default_value = "http://localhost:23151", long)]
        agent: String,

        #[arg(long)]
        alias: Option<String>,

        #[arg(default_value = "Vorpal.toml", long)]
        config: String,

        #[arg(default_value_t = default_artifact_context(), long)]
        context: String,

        #[arg(default_value_t = false, long)]
        export: bool,

        #[arg(long)]
        name: String,

        #[arg(default_value_t = false, long)]
        path: bool,

        #[arg(default_value_t = get_system_default_str(), long)]
        system: String,

        #[arg(long)]
        variable: Vec<String>,

        #[clap(default_value = "http://localhost:23151", long)]
        worker: String,
    },

    Init {},

    #[clap(subcommand)]
    Keys(CommandKeys),

    Start {
        #[clap(default_value = "23151", long)]
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
pub enum CommandKeys {
    Generate {},
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Log level
    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,

    /// Registry address
    #[clap(default_value = "http://localhost:23151", long)]
    registry: String,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    let Cli {
        command,
        level,
        registry,
    } = cli;

    match &command {
        Command::Artifact {
            agent,
            alias: artifact_alias,
            config: artifact_config,
            context: artifact_context,
            export: artifact_export,
            name: artifact_name,
            path: artifact_path,
            system: artifact_system,
            variable,
            worker,
        } => {
            artifact::run(
                agent,
                artifact_alias.clone(),
                artifact_config,
                artifact_context,
                *artifact_export,
                artifact_name,
                *artifact_path,
                artifact_system,
                level,
                &registry,
                variable.clone(),
                worker,
            )
            .await
        }

        Command::Init {} => init::run(level).await,

        Command::Keys(keys) => match keys {
            CommandKeys::Generate {} => keys::generate().await,
        },

        Command::Start {
            port,
            registry_backend,
            registry_backend_s3_bucket,
            services,
        } => {
            start::run(
                level,
                *port,
                registry.clone(),
                registry_backend.clone(),
                registry_backend_s3_bucket.clone(),
                services.split(',').map(|s| s.to_string()).collect(),
            )
            .await
        }
    }
}
