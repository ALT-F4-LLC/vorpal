use crate::{
    package::language::{build_rust_package, PackageRust},
    service::ContextConfig,
};
use anyhow::Result;
use vorpal_schema::vorpal::{
    config::v0::Config,
    package::v0::PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

mod cli;
mod cross_platform;
mod package;
mod sandbox;
mod service;

// Configuration function that returns a Config struct
fn build_config(context: &mut ContextConfig) -> Result<Config> {
    // TODO: add any custom logic you want here

    let vorpal_config = PackageRust {
        cargo_hash: "d64f6649f972632272a5cad4e24b1a3721c76de391dd7e6400b34b5d3050b52a",
        name: "vorpal",
        source: ".",
        source_excludes: vec![".env", ".packer", ".vagrant", "script"],
        systems: vec![Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos],
    };

    let vorpal = build_rust_package(context, vorpal_config)?;

    Ok(Config {
        packages: vec![vorpal],
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    cli::execute(build_config).await
}