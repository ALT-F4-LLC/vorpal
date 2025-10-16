use crate::artifact::get_default_address;
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
        #[clap(default_value_t = get_default_address(), long)]
        agent: String,

        #[clap(long)]
        artifact: String,

        #[clap(long)]
        artifact_context: String,

        #[clap(long)]
        artifact_namespace: String,

        #[arg(long)]
        artifact_system: String,

        #[clap(long, default_value_t = false)]
        artifact_unlock: bool,

        #[clap(long)]
        artifact_variable: Vec<String>,

        #[clap(long)]
        port: u16,

        #[clap(default_value_t = get_default_address(), long)]
        registry: String,
    },
}
