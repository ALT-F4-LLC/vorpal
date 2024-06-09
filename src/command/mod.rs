use crate::service::build;
use crate::service::proxy;
use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::Level;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[clap(long, global = true, default_value_t = Level::INFO)]
    pub level: tracing::Level,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[clap(subcommand)]
    Keys(Keys),

    #[clap(subcommand)]
    Services(Services),
}

#[derive(Subcommand)]
pub enum Keys {
    Generate {},
}

#[derive(Subcommand)]
pub enum Services {
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

    let mut subscriber = tracing_subscriber::FmtSubscriber::builder().with_max_level(cli.level);

    // when we run the command with `TRACE` or `DEBUG` level, we want to see
    // the file and line number...
    if [Level::DEBUG, Level::TRACE].contains(&cli.level) {
        subscriber = subscriber.with_file(true).with_line_number(true);
    }
    let subscriber = subscriber.finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber");

    match &cli.command {
        Command::Keys(keys) => match keys {
            Keys::Generate {} => crate::notary::generate_keys().await,
        },
        Command::Services(service) => match service {
            Services::Build(build) => match build {
                Build::Start { port } => build::start(*port).await,
            },
            Services::Proxy(proxy) => match proxy {
                Proxy::Start { port } => proxy::start(*port).await,
            },
        },
    }
}
