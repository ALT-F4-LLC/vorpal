use crate::{
    artifact::language::{build_rust_artifact, ArtifactRust},
    service::ContextConfig,
};
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

    let vorpal_config = ArtifactRust {
        // cargo_hash: "d64f6649f972632272a5cad4e24b1a3721c76de391dd7e6400b34b5d3050b52a",
        name: "vorpal",
        source: ".",
        source_excludes: vec![".env", ".packer", ".vagrant", "script"],
        systems: vec![Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos],
    };

    let vorpal = build_rust_artifact(context, vorpal_config)?;

    Ok(Config {
        artifacts: vec![vorpal],
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    cli::execute(build_config).await
}
