use crate::{
    cross_platform::get_cpu_count,
    package::{
        add_default_environment, add_default_script, linux_headers, native_binutils, native_gcc,
        native_glibc, native_libstdcpp, native_m4, native_zlib, BuildPackageOptionsEnvironment,
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
    let zlib_native = native_zlib::package(target)?;

    let name = "ncurses-native";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        mkdir build

        pushd build

        ../configure AWK=gawk

        make -C include

        make -C progs tic

        popd

        ./configure \
            --build=$(./config.guess) \
            --disable-stripping \
            --prefix=\"$output\" \
            --with-cxx-shared \
            --with-manpage-format=normal \
            --with-shared \
            --without-ada \
            --without-debug \
            --without-normal \
            AWK=gawk

        make -j$({cores})

        make TIC_PATH=$(pwd)/build/progs/tic install

        ln -sv libncursesw.so $LFS/usr/lib/libncurses.so

        sed -e 's/^#if.*XOPEN.*$/#if 1/' -i $LFS/usr/include/curses.h",
        source = name,
        cores = get_cpu_count(target)?,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("fd793cdfc421fac76f4af23c7d960cbe4a29cbb18f5badf37b85e16a894b3b6d".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://invisible-island.net/archives/ncurses/ncurses-6.5.tar.gz".to_string(),
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
            zlib_native,
        ],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let environment_options = BuildPackageOptionsEnvironment {
        binutils: true,
        gcc: true,
        glibc: false,
        libstdcpp: true,
        linux_headers: true,
        zlib: true,
    };

    let package = add_default_environment(package, Some(environment_options));

    let package = add_default_script(package, target, None)?;

    Ok(package)
}
