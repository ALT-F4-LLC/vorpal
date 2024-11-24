use crate::{service, ContextConfig};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env::consts::{ARCH, OS};
use tracing::Level;
use vorpal_schema::{
    get_artifact_system, vorpal::artifact::v0::ArtifactSystem, vorpal::config::v0::Config,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn get_default_system() -> String {
    format!("{}-{}", ARCH, OS)
}

#[derive(Subcommand)]
enum Command {
    Start {
        #[clap(default_value_t = Level::INFO, global = true, long)]
        level: Level,

        #[clap(long, short)]
        port: u16,

        #[arg(default_value_t = get_default_system(), long, short)]
        target: String,
    },
}

pub type ConfigFunction = fn(context: &mut ContextConfig) -> Result<Config>;

pub async fn execute(config: ConfigFunction) -> Result<()> {
    let args = Cli::parse();

    match args.command {
        Command::Start { port, target, .. } => {
            let target = get_artifact_system::<ArtifactSystem>(&target);

            if target == ArtifactSystem::UnknownSystem {
                return Err(anyhow::anyhow!("Invalid target system"));
            }

            let mut config_context = ContextConfig::new(target);

            let config = config(&mut config_context)?;

            service::listen(config_context, config, port).await?;
        }
    }

    Ok(())
}
