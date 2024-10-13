use crate::package::language::{build_rust_package, PackageRust};
use anyhow::Result;
use std::collections::HashMap;
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
fn build_config(target: PackageSystem) -> Config {
    // TODO: add any custom logic you want here

    let package = build_rust_package(
        PackageRust {
            cargo_hash: "38c665871adcb33761b255945fab59be4a225d8ff14834d2c3924a99aec49630",
            name: "vorpal",
            source: ".",
            source_excludes: vec![".env", ".packer", ".vagrant", "script"],
            systems: vec![Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos],
        },
        target,
    )
    .expect("Failed to build package");

    Config {
        packages: HashMap::from([("default".to_string(), package)]),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    cli::execute(build_config).await
}
