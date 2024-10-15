use crate::{
    cross_platform::get_cpu_count,
    package::{
        add_default_environment, add_default_script, BuildPackageOptionsEnvironment,
        BuildPackageOptionsScripts,
    },
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(target: PackageSystem) -> Result<Package> {
    let name = "zlib-native";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        ./configure --prefix=$output

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count(target)?,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("3f7995d5f103719283f509c23624287ce95c349439e881ed935a3c2c807bb683".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz"
            .to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let environment_options = BuildPackageOptionsEnvironment {
        binutils: false,
        gcc: false,
        glibc: false,
        zlib: false,
    };

    let package = add_default_environment(package, Some(environment_options));

    let script_options = BuildPackageOptionsScripts {
        sanitize_interpreters: false,
        sanitize_paths: true,
    };

    let package = add_default_script(package, target, Some(script_options))?;

    Ok(package)
}
