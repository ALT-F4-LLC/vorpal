use crate::package::build_package;
use anyhow::{bail, Result};
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub fn package(system: PackageSystem) -> Result<Package> {
    let name = "rust-std";

    let source_hash = match system {
        Aarch64Linux => "72d4917bb58b693b3f2c589746ed470645f96895ece3dd27f7055d3c3f7f7a79",
        Aarch64Macos => "0689a9b2dec87c272954db9212a8f3d5243f55f777f90d84d2b3aeb2aa938ba5",
        X8664Linux => "ad734eb9699b0a9dffdd35034776ccaa4d7b45e1898fc32748be93b60453550d",
        X8664Macos => "",
        UnknownSystem => bail!("Unsupported system: {:?}", system),
    };

    let source_target = match system {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported system: {:?}", system),
    };

    let source_version = "1.78.0";

    let source = PackageSource {
        excludes: vec![],
        hash: Some(source_hash.to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: format!(
            "https://static.rust-lang.org/dist/2024-05-02/rust-std-{}-{}.tar.gz",
            source_version, source_target
        ),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![],
        sandbox: true,
        script: format!(
            "cp -pr ./{}/{}-{}/* \"$output/.\"",
            name, name, source_target
        ),
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    build_package(package, system)
}
