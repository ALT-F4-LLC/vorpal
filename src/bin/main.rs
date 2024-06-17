use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::Level;
use vorpal::notary;
use vorpal::service::{build, proxy};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[clap(long, global = true, default_value_t = Level::INFO)]
    level: tracing::Level,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
enum Services {
    Proxy {
        #[clap(default_value = "15323", long, short)]
        port: u16,
    },

    Build {
        #[clap(default_value = "23151", long, short)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
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
            Keys::Generate {} => notary::generate_keys().await,
        },
        Command::Services(service) => match service {
            Services::Proxy { port } => proxy::start(*port).await,
            Services::Build { port } => build::start(*port).await,
        },
    }
}
