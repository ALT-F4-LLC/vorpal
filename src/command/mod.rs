use crate::service::build;
use crate::service::proxy;
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
    Service(Service),
}

#[derive(Subcommand)]
pub enum Service {
    #[clap(subcommand)]
    Build(Build),

    #[clap(subcommand)]
    Proxy(Proxy),
}

#[derive(Subcommand)]
pub enum Build {
    Start {
        #[clap(default_value = "15323", long, short)]
        port: u16,
    },
}

#[derive(Subcommand)]
pub enum Proxy {
    Start {
        #[clap(default_value = "23151", long, short)]
        port: u16,
    },
}

pub async fn run() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Service(service) => match service {
            Service::Build(build) => match build {
                Build::Start { port } => build::start(port.clone()).await,
            },
            Service::Proxy(proxy) => match proxy {
                Proxy::Start { port } => proxy::start(port.clone()).await,
            },
        },
    }
}
