use crate::{
    cross_platform::get_cpu_count,
    package::{
        add_default_environment, add_default_script, linux_headers, native_binutils, native_gcc,
        native_glibc, native_libstdcpp, native_m4, native_ncurses, native_zlib,
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
    let libstdcpp_native = native_libstdcpp::package(target)?;
    let linux_headers = linux_headers::package(target)?;
    let m4_native = native_m4::package(target)?;
    let ncurses_native = native_ncurses::package(target)?;
    let zlib_native = native_zlib::package(target)?;

    let name = "patchelf-native";

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
        hash: Some("a278eec544da9f0a82ad7e07b3670cf0f4d85ee13286fa9ad4f4416b700ac19d".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://github.com/NixOS/patchelf/releases/download/0.18.0/patchelf-0.18.0.tar.gz"
            .to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![
            binutils_native,
            gcc_native,
            glibc_native,
            libstdcpp_native,
            linux_headers,
            m4_native,
            ncurses_native,
            zlib_native,
        ],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(package, None);

    let package = add_default_script(package, target, None)?;

    Ok(package)
}
