use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub fn package(system: PackageSystem) -> Result<Package> {
    let name = "zstd-native";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        make -j$({cores})
        make install PREFIX=\"$output\"",
        source = name,
        cores = get_cpu_count()?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("7ad49c2d889f82744c95206e9fe38ecde31fc116d1dad8def68776c69887efcf".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://github.com/facebook/zstd/releases/download/v1.5.5/zstd-1.5.5.tar.gz"
            .to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    let package = add_default_environment(package, None);
    let package = add_default_script(package, system, None)?;

    Ok(package)
}
