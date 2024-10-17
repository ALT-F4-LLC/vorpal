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

#[allow(clippy::too_many_arguments)]
pub fn package(
    target: PackageSystem,
    binutils: Package,
    gcc: Package,
    glibc: Package,
    libstdcpp: Package,
    linux_headers: Package,
    m4: Package,
    zlib: Package,
) -> Result<Package> {
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

        ln -sv libncursesw.so $output/lib/libncurses.so

        sed -e 's/^#if.*XOPEN.*$/#if 1/' -i $output/include/ncursesw/curses.h",
        source = name,
        cores = get_cpu_count(target)?,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("aab234a3b7a22e2632151fbe550cb36e371d3ee5318a633ee43af057f9f112fb".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://invisible-island.net/archives/ncurses/ncurses-6.5.tar.gz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![
            binutils.clone(),
            gcc.clone(),
            glibc.clone(),
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
            zlib.clone(),
        ],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(
        package,
        None,
        Some(binutils),
        Some(gcc),
        Some(glibc.clone()),
        Some(libstdcpp),
        Some(linux_headers),
        Some(m4),
        Some(zlib),
    );

    let package = add_default_script(package, target, Some(glibc))?;

    Ok(package)
}
