use clap::{Parser, Subcommand};
use tracing::Level;
use vorpal_schema::system_default_str;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Start {
        #[clap(default_value = "http://localhost:23151", long, short)]
        agent: String,

        #[clap(default_value_t = Level::INFO, global = true, long)]
        level: Level,

        #[clap(long, short)]
        port: u16,

        #[clap(default_value = "http://localhost:23151", long, short)]
        registry: String,

        #[arg(default_value_t = system_default_str(), long, short)]
        target: String,
    },
}
