use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script, native_gcc, native_glibc},
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub fn package(system: PackageSystem) -> Result<Package> {
    let gcc = native_gcc::package(system)?;
    let glibc = native_glibc::package(system)?;

    let name = "patchelf-native";

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
        hash: Some("a278eec544da9f0a82ad7e07b3670cf0f4d85ee13286fa9ad4f4416b700ac19d".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://github.com/NixOS/patchelf/releases/download/0.18.0/patchelf-0.18.0.tar.gz"
            .to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![gcc, glibc],
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
