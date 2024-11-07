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
        cargo_hash: "a763b89e7a0fa55e18aba19b797fb47a556814f02b858b1d9142fa60d473a88f",
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
