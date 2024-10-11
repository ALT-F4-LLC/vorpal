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
    let name = "coreutils-native";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        ./configure --prefix=\"$output\"

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count()?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz".to_string(),
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
