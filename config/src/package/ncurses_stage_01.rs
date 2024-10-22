use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

#[allow(clippy::too_many_arguments)]
pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    binutils: &PackageOutput,
    gcc: &PackageOutput,
    glibc: &PackageOutput,
    libstdcpp: &PackageOutput,
    linux_headers: &PackageOutput,
    m4: &PackageOutput,
    zlib: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "ncurses-stage-01";

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
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://invisible-island.net/archives/ncurses/ncurses-6.5.tar.gz".to_string(),
    };

    let package = Package {
        environment: vec![],
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
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(
        package,
        None,
        Some(binutils),
        Some(gcc),
        None,
        Some(libstdcpp),
        Some(linux_headers),
        Some(m4),
        Some(zlib),
    );

    let package = add_default_script(package, target, None, Some(glibc))?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
