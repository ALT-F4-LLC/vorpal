use crate::{
    cross_platform::get_cpu_count,
    package::{
        add_default_environment, add_default_script, native_glibc, BuildPackageOptionsEnvironment,
    },
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub fn package(system: PackageSystem) -> Result<Package> {
    let glibc = native_glibc::package(system)?;

    let name = "gcc-native";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        ./contrib/download_prerequisites

        case $(uname -m) in
          x86_64)
            sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
         ;;
        esac

        mkdir -p build

        cd ./build

        ../configure \
          --disable-bootstrap \
          --disable-fixincludes \
          --disable-multilib \
          --enable-default-pie \
          --enable-default-ssp \
          --enable-host-pie \
          --enable-languages=c,c++ \
          --prefix=\"$output\" \
          --with-system-zlib

        make -j$({cores})
        make install

        ln -s $output/bin/gcc $output/bin/cc",
        source = name,
        cores = get_cpu_count()?
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
        packages: vec![glibc],
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

    let environment_options = BuildPackageOptionsEnvironment {
        gcc: false,
        glibc: false,
    };

    let package = add_default_environment(package, Some(environment_options));
    let package = add_default_script(package, system, None)?;

    Ok(package)
}
