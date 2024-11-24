use crate::{artifact::language::rust::build_artifact, service::ContextConfig};
use anyhow::Result;
use vorpal_schema::vorpal::{
    artifact::v0::ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
    config::v0::Config,
};

mod artifact;
mod cli;
mod cross_platform;
mod sandbox;
mod service;

// Configuration function that returns a Config struct
fn build_config(context: &mut ContextConfig) -> Result<Config> {
    // TODO: add any custom logic you want here

    // Define the Rust artifact parameters
    let cargo_hash = "59324cc6fb0c81f0ab5ae77c235b3a0060eadaa7e9b0277aa74fbdcc9b839463";
    let excludes = vec![".env", ".packer", ".vagrant", "script"];
    let name = "vorpal";
    let systems = vec![Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos];

    // Build the Rust artifact
    let vorpal = build_artifact(context, cargo_hash, excludes, name, systems)?;

    // Return the Config struct
    Ok(Config {
        artifacts: vec![vorpal],
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    cli::execute(build_config).await
}
