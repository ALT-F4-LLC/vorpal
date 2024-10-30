use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
    sandbox::{add_default_host_paths, SandboxDefaultPaths},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSource, PackageSystem,
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

    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let sandbox_paths = SandboxDefaultPaths {
        autoconf: true,
        automake: true,
        bash: true,
        binutils: false,
        bison: true,
        bzip2: true,
        coreutils: true,
        curl: true,
        diffutils: true,
        file: true,
        findutils: true,
        flex: false,
        gawk: true,
        gcc: false,
        gcc_12: false,
        glibc: false,
        grep: true,
        gzip: true,
        help2man: true,
        includes: true,
        lib: true,
        m4: true,
        make: true,
        patchelf: true,
        perl: true,
        python: true,
        sed: true,
        tar: true,
        texinfo: true,
        wget: true,
    };

    let sandbox = PackageSandbox {
        paths: add_default_host_paths(sandbox_paths),
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
        environment,
        name: name.to_string(),
        packages: vec![
            binutils.clone(),
            gcc.clone(),
            glibc.clone(),
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
        ],
        sandbox: Some(sandbox),
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
        None,
        None,
    );

    let package = add_default_script(package, target, Some(glibc))?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
