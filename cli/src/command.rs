use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env::current_dir;
use tracing::{subscriber, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};
use vorpal_sdk::artifact::system::get_system_default_str;

mod artifact;
mod init;
mod start;
mod store;
mod system;

fn default_artifact_context() -> String {
    current_dir()
        .expect("failed to get current directory")
        .to_string_lossy()
        .to_string()
}

#[derive(Subcommand)]
pub enum CommandArtifact {
    Inspect {
        /// Artifact digest to inspect
        digest: String,
    },

    Make {
        #[clap(default_value = "http://localhost:23151", long)]
        agent: String,

        #[arg(long)]
        alias: Vec<String>,

        #[arg(default_value = "Vorpal.toml", long)]
        config: String,

        #[arg(default_value_t = default_artifact_context(), long)]
        context: String,

        #[arg(default_value_t = false, long)]
        export: bool,

        /// Artifact name
        name: String,

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

        #[clap(default_value = "http://localhost:23151", long)]
        worker: String,
    },
}

#[derive(Subcommand)]
pub enum CommandSystemKeys {
    Generate {},
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

    Init {},

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

    #[clap(subcommand)]
    System(CommandSystem),
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
            CommandArtifact::Inspect { digest } => {
                artifact::inspect::run(digest, level, &registry).await
            }

            CommandArtifact::Make {
                agent,
                alias: artifact_aliases,
                config: artifact_config,
                context: artifact_context,
                export: artifact_export,
                lockfile_update: artifact_lockfile_update,
                name: artifact_name,
                path: artifact_path,
                rebuild: artifact_rebuild,
                system: artifact_system,
                variable,
                worker,
            } => {
                artifact::make::run(
                    agent,
                    artifact_aliases.clone(),
                    artifact_config,
                    artifact_context,
                    *artifact_export,
                    *artifact_lockfile_update,
                    artifact_name,
                    *artifact_path,
                    *artifact_rebuild,
                    artifact_system,
                    &registry,
                    variable.clone(),
                    worker,
                )
                .await
            }
        },

        Command::Init {} => init::run(level).await,

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

        Command::Start {
            port,
            registry_backend,
            registry_backend_s3_bucket,
            services,
        } => {
            start::run(
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
