use crate::package::{
    add_default_environment, add_default_script, native_binutils, native_gcc, native_zlib,
    BuildPackageOptionsEnvironment, BuildPackageOptionsScripts,
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(target: PackageSystem) -> Result<Package> {
    let binutils_native = native_binutils::package(target)?;
    let gcc_native = native_gcc::package(target)?;
    let zlib_native = native_zlib::package(target)?;

    let name = "linux-headers";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        make mrproper
        make headers

        find usr/include -type f ! -name '*.h' -delete

        mkdir -p \"$output/usr\"
        cp -rv usr/include \"$output/usr\"",
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("3fa3f4f3d010de5b9bde09d08a251fa3ef578d356d3a7a29b6784a6916ea0d50".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.10.8.tar.xz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![binutils_native, gcc_native, zlib_native],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let environment_options = BuildPackageOptionsEnvironment {
        binutils: true,
        gcc: true,
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
