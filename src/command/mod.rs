use crate::builder;
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[clap(subcommand)]
    Builder(Builder),
}

#[derive(Subcommand)]
pub enum Builder {
    Start {
        #[clap(default_value = "15323", long, short)]
        port: u16,
    },
}

pub async fn run() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Builder(Builder::Start { port }) => builder::start(port.clone()).await,
    }
}
