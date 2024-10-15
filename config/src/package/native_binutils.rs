use crate::{
    cross_platform::get_cpu_count,
    package::{
        add_default_environment, add_default_script, native_zlib, BuildPackageOptionsEnvironment,
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
    let zlib_native = native_zlib::package(target)?;

    let name = "binutils-native-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        mkdir -p \"${{PWD}}/{source}\"/build
        cd \"${{PWD}}/{source}\"/build

        ../configure \
            --disable-nls \
            --disable-werror \
            --enable-default-hash-style=\"gnu\" \
            --enable-gprofng=\"no\" \
            --enable-new-dtags \
            --prefix=\"$output\"

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![zlib_native],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let environment_options = BuildPackageOptionsEnvironment {
        binutils: false,
        gcc: false,
        glibc: false,
        libstdcpp: false,
        linux_headers: false,
        zlib: true,
    };

    let package = add_default_environment(package, Some(environment_options));

    let script_options = BuildPackageOptionsScripts {
        sanitize_interpreters: false,
        sanitize_paths: true,
    };

    let package = add_default_script(package, target, Some(script_options))?;

    Ok(package)
}
