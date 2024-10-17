use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(target: PackageSystem, zlib: Package) -> Result<Package> {
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
        packages: vec![zlib.clone()],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(
        package,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(zlib),
    );

    let package = add_default_script(package, target, None)?;

    Ok(package)
}
