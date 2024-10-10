use crate::package::{build_package, rust_std};
use anyhow::{bail, Result};
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub fn package(system: PackageSystem) -> Result<Package> {
    let rust_std = rust_std::package(system)?;

    let name = "rustc";

    let script = formatdoc! {"
        cp -pr ./rustc/rustc/* \"$output/.\"
        cat \"$rust_std/manifest.in\" >> \"$output/manifest.in\"
        cp -pr \"$rust_std/lib\" \"$output\"
        "
    };

    let hash = match system {
        Aarch64Linux => "bc6c0e0f309805c4a9b704bbfe6be6b3c28b029ac6958c58ab5b90437a9e36ed",
        Aarch64Macos => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
        X8664Linux => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
        X8664Macos => "",
        UnknownSystem => bail!("Unsupported system: {:?}", system),
    };

    let target = match system {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported system: {:?}", system),
    };

    let version = "1.78.0";

    let source = PackageSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: format!(
            "https://static.rust-lang.org/dist/2024-05-02/rustc-{}-{}.tar.gz",
            version, target
        ),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![rust_std],
        sandbox: true,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    build_package(package, system, None)
}
