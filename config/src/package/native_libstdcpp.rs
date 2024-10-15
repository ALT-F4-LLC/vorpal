use crate::{
    cross_platform::get_cpu_count,
    package::{
        add_default_environment, add_default_script, linux_headers, native_binutils, native_gcc,
        native_glibc, native_zlib, BuildPackageOptionsScripts,
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
    let binutils_native = native_binutils::package(target)?;
    let gcc_native = native_gcc::package(target)?;
    let glibc_native = native_glibc::package(target)?;
    let linux_headers = linux_headers::package(target)?;
    let zlib_native = native_zlib::package(target)?;

    let name = "libstdcpp-native-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        mkdir -p build

        cd build

        ../libstdc++-v3/configure \
            --build=$(../config.guess) \
            --disable-libstdcxx-pch \
            --disable-multilib \
            --disable-nls \
            --prefix=\"$output\" \
            --with-gxx-include-dir=\"$gcc_native_stage_01/include/c++/14.2.0\"

        make -j$({cores})
        make install

        rm -v $output/usr/lib/lib{{stdc++{{,exp,fs}},supc++}}.la",
        source = name,
        cores = get_cpu_count(target)?,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![
            binutils_native,
            gcc_native,
            glibc_native,
            linux_headers,
            zlib_native,
        ],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(package, None);

    let script_options = BuildPackageOptionsScripts {
        sanitize_interpreters: false,
        sanitize_paths: true,
    };

    let package = add_default_script(package, target, Some(script_options))?;

    Ok(package)
}
