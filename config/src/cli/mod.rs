use crate::service;
use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::Level;
use vorpal_schema::vorpal::{config::v0::Config, package::v0::PackageSystem};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Start {
        #[clap(default_value_t = Level::INFO, global = true, long)]
        level: Level,

        #[clap(long, short)]
        port: u16,
    },
}

pub type BuildConfigFn = fn(system: PackageSystem) -> Config;

pub async fn execute(config: BuildConfigFn) -> Result<()> {
    let args = Cli::parse();

    match args.command {
        Command::Start { port, .. } => {
            service::listen(config, port).await?;
        }
    }

    Ok(())
}
