use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script, native_glibc, native_patchelf},
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub fn package(target: PackageSystem) -> Result<Package> {
    let name = "bash-native";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        ./configure --prefix=\"$output\"

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz".to_string(),
    };

    let mut packages = vec![];

    if target == Aarch64Linux || target == X8664Linux {
        packages.push(native_glibc::package(target)?);
        packages.push(native_patchelf::package(target)?);
    }

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages,
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
    let package = add_default_script(package, target, None)?;

    Ok(package)
}
