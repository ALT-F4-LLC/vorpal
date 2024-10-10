use crate::{package::build_config, service};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use tracing::Level;
use vorpal_schema::vorpal::package::v0::Package;

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

pub async fn execute(packages: HashMap<String, Package>) -> Result<()> {
    let args = Cli::parse();

    match args.command {
        Command::Start { port, .. } => {
            let config = build_config(packages);

            service::listen(config, port).await?;
        }
    }

    Ok(())
}
