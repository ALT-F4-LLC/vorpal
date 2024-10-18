use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

#[allow(clippy::too_many_arguments)]
pub fn package(
    target: PackageSystem,
    binutils: Option<Package>,
    gcc: Option<Package>,
    glibc: Option<Package>,
    libstdcpp: Option<Package>,
    linux_headers: Option<Package>,
    m4: Option<Package>,
    ncurses: Option<Package>,
    zlib: Option<Package>,
) -> Result<Package> {
    let name = "bash-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        ./configure \
            --build=$(sh support/config.guess) \
            --prefix=\"$output\" \
            --without-bash-malloc \
            bash_cv_strtold_broken=\"no\"

        make -j$({cores})
        make install

        ln -s $output/bin/bash $output/bin/sh",
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz".to_string(),
    };

    let mut packages = vec![];

    if target == Aarch64Linux || target == X8664Linux {
        if let Some(binutils) = &binutils {
            packages.push(binutils.clone());
        }

        if let Some(gcc) = &gcc {
            packages.push(gcc.clone());
        }

        if let Some(glibc) = &glibc {
            packages.push(glibc.clone());
        }

        if let Some(libstdcpp) = &libstdcpp {
            packages.push(libstdcpp.clone());
        }

        if let Some(linux_headers) = &linux_headers {
            packages.push(linux_headers.clone());
        }

        if let Some(m4) = &m4 {
            packages.push(m4.clone());
        }

        if let Some(ncurses) = &ncurses {
            packages.push(ncurses.clone());
        }

        if let Some(zlib) = &zlib {
            packages.push(zlib.clone());
        }
    }

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages,
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

    let package = add_default_environment(
        package,
        None,
        binutils,
        gcc,
        None,
        libstdcpp,
        linux_headers,
        ncurses,
        zlib,
    );

    let package = add_default_script(package, target, glibc)?;

    Ok(package)
}
