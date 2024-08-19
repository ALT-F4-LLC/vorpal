use crate::service::start;
use anyhow::Result;
use clap::Parser;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

pub mod package;
pub mod service;
pub mod store;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[clap(long, global = true, default_value_t = Level::INFO)]
    level: tracing::Level,

    #[clap(default_value = "23151", long, short)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let mut subscriber = FmtSubscriber::builder().with_max_level(cli.level);

    let Cli { port, level } = cli;

    // when we run the command with `TRACE` or `DEBUG` level, we want to see
    // the file and line number...
    if [Level::DEBUG, Level::TRACE].contains(&level) {
        subscriber = subscriber.with_file(true).with_line_number(true);
    }

    let subscriber = subscriber.finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber");

    start(port).await?;

    Ok(())
}
