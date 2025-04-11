use crate::system::get_system_default_str;
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
    Start {
        #[clap(default_value = "http://localhost:23151", long)]
        agent: String,

        #[clap(long)]
        port: u16,

        #[clap(default_value = "http://localhost:23151", long)]
        registry: String,

        #[arg(default_value_t = get_system_default_str(), long)]
        target: String,
    },
}
