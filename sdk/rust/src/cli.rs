use crate::artifact::get_default_address;
use clap::{Parser, Subcommand};

/// Command-line interface for a Vorpal artifact config binary.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands accepted by a Vorpal artifact config binary.
#[derive(Subcommand)]
pub enum Command {
    /// Build the artifacts defined by this config against a running agent.
    Start {
        /// Address of the Vorpal agent service to build against.
        #[clap(default_value_t = get_default_address(), long)]
        agent: String,

        /// Name of the artifact to build.
        #[clap(long)]
        artifact: String,

        /// Path to the artifact's source context directory.
        #[clap(long)]
        artifact_context: String,

        /// Namespace the artifact belongs to.
        #[clap(long)]
        artifact_namespace: String,

        /// Target system the artifact is built for.
        #[arg(long)]
        artifact_system: String,

        /// Skip failing the build when a source's content digest no longer
        /// matches its pinned digest.
        #[clap(long, default_value_t = false)]
        artifact_unlock: bool,

        /// Variable overrides passed to the artifact build, as `key=value` pairs.
        #[clap(long)]
        artifact_variable: Vec<String>,

        /// Port the local `ContextService` gRPC server listens on.
        #[clap(long)]
        port: u16,

        /// Address of the Vorpal registry service used to fetch and store artifacts.
        #[clap(default_value_t = get_default_address(), long)]
        registry: String,
    },
}
