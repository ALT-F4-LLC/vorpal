use crate::{
    cross_platform::get_cpu_count,
    sandbox::{
        environments::add_environments,
        paths::{add_paths, SandboxDefaultPaths},
        scripts::{add_scripts, PackageRpath},
    },
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
    make: &PackageOutput,
    ncurses: &PackageOutput,
    patch: &PackageOutput,
    sed: &PackageOutput,
    tar: &PackageOutput,
) -> Result<PackageOutput> {
    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let name = "xz-stage-01";

    let sandbox_paths = SandboxDefaultPaths {
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
        make: false,
        patchelf: true,
        perl: true,
        python: true,
        sed: false,
        tar: false,
        texinfo: true,
        wget: true,
    };

    let sandbox = PackageSandbox {
        paths: add_paths(sandbox_paths),
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
            --disable-static

        make -j$({cores})
        make install

        rm -v $output/lib/liblzma.la",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        m4 = m4.name.to_lowercase().replace("-", "_"),
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("7a02b1278ed9a59b332657d613c5549b39afe34e315197f4da95c5322524ec26".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://github.com//tukaani-project/xz/releases/download/v5.6.2/xz-5.6.2.tar.xz"
            .to_string(),
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
            make.clone(),
            m4.clone(),
            ncurses.clone(),
            patch.clone(),
            sed.clone(),
            tar.clone(),
        ],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_environments(
        package,
        Some(bash),
        Some(binutils),
        Some(gcc),
        None,
        Some(libstdcpp),
        Some(linux_headers),
        Some(ncurses),
    );

    let glibc_env_key = glibc.name.to_lowercase().replace("-", "_");

    let package_rpaths = vec![PackageRpath {
        rpath: format!("${}/lib", glibc_env_key),
        shrink: false,
        target: "$output".to_string(),
    }];

    let package = add_scripts(package, target, Some(glibc), package_rpaths)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
