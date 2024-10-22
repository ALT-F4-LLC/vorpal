use crate::{
    cross_platform::{get_cpu_count, get_sed_cmd},
    package::{add_default_environment, add_default_script},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

#[allow(clippy::too_many_arguments)]
pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    bash: &PackageOutput,
    binutils: Option<&PackageOutput>,
    gcc: Option<&PackageOutput>,
    glibc: Option<&PackageOutput>,
    libstdcpp: Option<&PackageOutput>,
    linux_headers: Option<&PackageOutput>,
    m4: Option<&PackageOutput>,
    ncurses: Option<&PackageOutput>,
    zlib: Option<&PackageOutput>,
) -> Result<PackageOutput> {
    let name = "coreutils-stage-01";

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        ./configure \
            --build=$(build-aux/config.guess) \
            --enable-install-program=\"hostname\" \
            --enable-no-install-program=\"kill,uptime\" \
            --prefix=\"$output\"

        make -j$({cores})
        make install

        mkdir -pv $output/sbin

        mv -v $output/bin/chroot $output/sbin/chroot

        mkdir -pv $output/share/man/man8

        mv -v $output/share/man/man1/chroot.1 \
            $output/share/man/man8/chroot.8

        {sed} 's/\"1\"/\"8\"/' $output/share/man/man8/chroot.8",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        sed = get_sed_cmd(target)?,
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz".to_string(),
    };

    let mut packages = vec![bash.clone()];

    if target == Aarch64Linux || target == X8664Linux {
        if let Some(binutils) = binutils {
            packages.push(binutils.clone());
        }

        if let Some(gcc) = gcc {
            packages.push(gcc.clone());
        }

        if let Some(glibc) = glibc {
            packages.push(glibc.clone());
        }

        if let Some(libstdcpp) = libstdcpp {
            packages.push(libstdcpp.clone());
        }

        if let Some(linux_headers) = linux_headers {
            packages.push(linux_headers.clone());
        }

        if let Some(m4) = m4 {
            packages.push(m4.clone());
        }

        if let Some(ncurses) = ncurses {
            packages.push(ncurses.clone());
        }

        if let Some(zlib) = zlib {
            packages.push(zlib.clone());
        }
    }

    let package = Package {
        environment: vec![],
        name: name.to_string(),
        packages,
        sandbox: false,
        script,
        source: vec![source],
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    let package = add_default_environment(
        package,
        Some(bash),
        binutils,
        gcc,
        None,
        libstdcpp,
        linux_headers,
        ncurses,
        zlib,
    );

    let package = add_default_script(package, target, glibc)?;

    let package_output = context.add_package(package.clone())?;

    Ok(package_output)
}
