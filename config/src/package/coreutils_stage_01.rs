use crate::{
    cross_platform::{get_cpu_count, get_sed_cmd},
    package::{add_default_environment, add_default_script},
    sandbox::{add_default_host_paths, SandboxDefaultPaths},
    ContextConfig,
};
use anyhow::{anyhow, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

fn get_error(package: &str) -> String {
    format!("The {} package is required for coreutils-stage-01", package)
}

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
) -> Result<PackageOutput> {
    let mut environment = vec![];
    let mut packages = vec![bash.clone()];
    let mut sandbox = None;

    let mut script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        mkdir -pv /bin

        ln -s ${bash}/bin/bash /bin/bash
        ln -s ${bash}/bin/bash /bin/sh",
        bash = bash.name.to_lowercase().replace("-", "_"),
    };

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
            bash: false,
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
            make: true,
            m4: false,
            patchelf: true,
            perl: true,
            python: true,
            sed: true,
            tar: true,
            texinfo: true,
            wget: true,
        };

        sandbox = Some(PackageSandbox {
            paths: add_default_host_paths(sandbox_paths),
        });

        let script_linux = formatdoc! {"\nln -s \"${m4}/bin/m4\" /usr/bin/m4\n",
            m4 = m4.name.to_lowercase().replace("-", "_"),
        };

        script.push_str(&script_linux);
    }

    let name = "coreutils-stage-01";

    let source = PackageSource {
        excludes: vec![],
        hash: Some("af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz".to_string(),
    };

    let script_install = formatdoc! {"
        cd \"${{PWD}}/{source}\"

        ./configure \
            --build=$(./build-aux/config.guess) \
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
        cores = get_cpu_count(target)?,
        sed = get_sed_cmd(target)?,
        source = name,
    };

    script.push_str(&script_install);

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

    let package = add_default_environment(
        package,
        Some(bash),
        binutils,
        gcc,
        None,
        libstdcpp,
        linux_headers,
        ncurses,
        None,
    );

    let package = add_default_script(package, target, glibc)?;

    let package_output = context.add_package(package.clone())?;

    Ok(package_output)
}
