use crate::{
    package::language::{build_rust_package, PackageRust},
    service::ContextConfig,
};
use anyhow::Result;
use vorpal_schema::vorpal::{
    config::v0::Config,
    package::v0::{
        PackageSystem,
        PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
    },
};

mod cli;
mod cross_platform;
mod package;
mod service;

// Configuration function that returns a Config struct
fn build_config(context: &mut ContextConfig, target: PackageSystem) -> Result<Config> {
    // TODO: add any custom logic you want here

    let vorpal_config = PackageRust {
        cargo_hash: "38c665871adcb33761b255945fab59be4a225d8ff14834d2c3924a99aec49630",
        name: "vorpal",
        source: ".",
        source_excludes: vec![".env", ".packer", ".vagrant", "script"],
        systems: vec![Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos],
    };

    let vorpal = build_rust_package(context, vorpal_config, target)?;

    Ok(Config {
        packages: vec![vorpal],
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    cli::execute(build_config).await
}
