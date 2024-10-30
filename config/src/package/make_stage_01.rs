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
    bash: &PackageOutput,
    binutils: &PackageOutput,
    coreutils: &PackageOutput,
    diffutils: &PackageOutput,
    file: &PackageOutput,
    findutils: &PackageOutput,
    gawk: &PackageOutput,
    gcc: &PackageOutput,
    glibc: &PackageOutput,
    grep: &PackageOutput,
    gzip: &PackageOutput,
    libstdcpp: &PackageOutput,
    linux_headers: &PackageOutput,
    m4: &PackageOutput,
    ncurses: &PackageOutput,
) -> Result<PackageOutput> {
    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let name = "make-stage-01";

    let sandbox_paths = SandboxDefaultPaths {
        autoconf: true,
        automake: true,
        bash: false,
        binutils: false,
        bison: true,
        bzip2: true,
        coreutils: false,
        curl: true,
        diffutils: false,
        file: false,
        findutils: false,
        flex: false,
        gawk: false,
        gcc: false,
        gcc_12: false,
        glibc: false,
        grep: false,
        gzip: false,
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

    let sandbox = PackageSandbox {
        paths: add_default_host_paths(sandbox_paths),
    };

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        mkdir -pv /bin

        ln -s ${bash}/bin/bash /bin/bash
        ln -s ${bash}/bin/bash /bin/sh
        ln -s ${m4}/bin/m4 /usr/bin/m4

        cd \"${{PWD}}/{source}\"

        ./configure \
            --prefix=\"$output\" \
            --build=$(./build-aux/config.guess) \
             --without-guile

        make -j$({cores})
        make install",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        m4 = m4.name.to_lowercase().replace("-", "_"),
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("8dfe7b0e51b3e190cd75e046880855ac1be76cf36961e5cfcc82bfa91b2c3ba8".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/make/make-4.4.1.tar.gz".to_string(),
    };

    let package = Package {
        environment,
        name: name.to_string(),
        packages: vec![
            bash.clone(),
            binutils.clone(),
            coreutils.clone(),
            diffutils.clone(),
            file.clone(),
            findutils.clone(),
            gawk.clone(),
            gcc.clone(),
            glibc.clone(),
            grep.clone(),
            gzip.clone(),
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
            ncurses.clone(),
        ],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(
        package,
        Some(bash),
        Some(binutils),
        Some(gcc),
        None,
        Some(libstdcpp),
        Some(linux_headers),
        Some(ncurses),
        None,
    );

    let package = add_default_script(package, target, Some(glibc))?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
