use crate::{
    cross_platform::get_cpu_count,
    package::{
        add_default_environment, add_default_script, linux_headers, native_binutils, native_gcc,
        native_zlib, BuildPackageOptionsEnvironment,
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
    let linux_headers = linux_headers::package(target)?;
    let zlib_native = native_zlib::package(target)?;

    let name = "glibc-native-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        mkdir -p \"${{PWD}}/{source}/build\"
        cd \"${{PWD}}/{source}/build\"
        
        echo \"rootsbindir=$output/sbin\" > configparms

        ../configure \
            --build=$(../scripts/config.guess) \
            --disable-nscd \
            --prefix=\"$output\" \
            --with-binutils=\"$binutils_native_stage_01/bin\" \
            --with-headers=\"$linux_headers/usr/include\" \
            libc_cv_slibdir=\"$output/lib\"

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/glibc/glibc-2.40.tar.gz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![binutils_native, gcc_native, linux_headers, zlib_native],
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
        linux_headers: true,
        zlib: true,
    };

    let package = add_default_environment(package, Some(environment_options));

    let package = add_default_script(package, target, None)?;

    Ok(package)
}
