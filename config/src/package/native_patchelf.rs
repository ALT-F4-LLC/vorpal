use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{Package, PackageSource, PackageSystem};

pub fn package(system: PackageSystem) -> Result<Package> {
    let name = "patchelf-native";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        ./bootstrap.sh
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
        packages: vec![],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![
            PackageSystem::Aarch64Linux.into(),
            PackageSystem::Aarch64Macos.into(),
            PackageSystem::X8664Linux.into(),
            PackageSystem::X8664Macos.into(),
        ],
    };

    let package = add_default_environment(package);
    let package = add_default_script(package, system)?;

    Ok(package)
}
