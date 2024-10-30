use crate::{package::build_package, ContextConfig};
use anyhow::{bail, Result};
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub fn package(context: &mut ContextConfig, system: PackageSystem) -> Result<PackageOutput> {
    let name = "cargo";

    let hash = match system {
        Aarch64Linux => "d782e34151df01519de86f0acace8a755cae6fad93cb0303ddd61c2642444c1c",
        Aarch64Macos => "d8ed8e9f5ceefcfe3bca7acd0797ade24eadb17ddccaa319cd00ea290f598d00",
        X8664Linux => "d8ed8e9f5ceefcfe3bca7acd0797ade24eadb17ddccaa319cd00ea290f598d00",
        X8664Macos => "",
        UnknownSystem => bail!("Unsupported cargo system: {:?}", system),
    };

    let target = match system {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported cargo target: {:?}", system),
    };

    let version = "1.78.0";

    let source = PackageSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: format!(
            "https://static.rust-lang.org/dist/2024-05-02/cargo-{}-{}.tar.gz",
            version, target
        ),
    };

    let package = Package {
        environment: vec![],
        name: name.to_string(),
        packages: vec![],
        sandbox: None,
        script: format!("cp -pr ./{}/{}/* \"$output/.\"", name, name),
        source: vec![source],
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    build_package(context, package, system)
}
