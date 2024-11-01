use crate::{
    cross_platform::get_cpu_count,
    sandbox::{
        environments::add_environments,
        paths::{add_paths, SandboxDefaultPaths},
        scripts::{add_scripts, PackageRpath},
    },
    ContextConfig,
};
use anyhow::{anyhow, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

fn get_error(package: &str) -> String {
    format!("The {} package is required for bash-stage-01", package)
}

#[allow(clippy::too_many_arguments)]
pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    binutils: Option<&PackageOutput>,
    gcc: Option<&PackageOutput>,
    glibc: Option<&PackageOutput>,
    libstdcpp: Option<&PackageOutput>,
    linux_headers: Option<&PackageOutput>,
    m4: Option<&PackageOutput>,
    ncurses: Option<&PackageOutput>,
) -> Result<PackageOutput> {
    let mut environment = vec![];
    let mut packages = vec![];
    let mut packages_rpaths = vec![];
    let mut sandbox = None;

    if target == Aarch64Linux || target == X8664Linux {
        environment.push(PackageEnvironment {
            key: "PATH".to_string(),
            value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
        });

        let binutils = binutils.ok_or_else(|| anyhow!(get_error("binutils")))?;
        let gcc = gcc.ok_or_else(|| anyhow!(get_error("gcc")))?;
        let glibc = glibc.ok_or_else(|| anyhow!(get_error("glibc")))?;
        let libstdcpp = libstdcpp.ok_or_else(|| anyhow!(get_error("libstdc++")))?;
        let linux_headers = linux_headers.ok_or_else(|| anyhow!(get_error("linux-headers")))?;
        let m4 = m4.ok_or_else(|| anyhow!(get_error("m4")))?;
        let ncurses = ncurses.ok_or_else(|| anyhow!(get_error("ncurses")))?;

        packages.push(binutils.clone());
        packages.push(gcc.clone());
        packages.push(glibc.clone());
        packages.push(libstdcpp.clone());
        packages.push(linux_headers.clone());
        packages.push(m4.clone());
        packages.push(ncurses.clone());

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
            m4: false,
            make: true,
            patchelf: true,
            perl: true,
            python: true,
            sed: true,
            tar: true,
            texinfo: true,
            wget: true,
        };

        sandbox = Some(PackageSandbox {
            paths: add_paths(sandbox_paths),
        });

        let glibc_env_key = glibc.name.to_lowercase().replace("-", "_");
        let ncurses_env_key = ncurses.name.to_lowercase().replace("-", "_");

        packages_rpaths.push(PackageRpath {
            rpath: format!("${}/lib:${}/lib", ncurses_env_key, glibc_env_key),
            shrink: false,
            target: "$output".to_string(),
        });
    }

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

        cp $output/bin/bash $output/bin/sh",
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz".to_string(),
    };

    let package = Package {
        environment,
        name: name.to_string(),
        packages,
        sandbox,
        script,
        source: vec![source],
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    let package = add_environments(
        package,
        None,
        binutils,
        gcc,
        None,
        libstdcpp,
        linux_headers,
        ncurses,
    );

    let package = add_scripts(package, target, glibc, packages_rpaths)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
