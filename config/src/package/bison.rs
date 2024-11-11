use crate::{
    cross_platform::get_cpu_count,
    sandbox::scripts::{add_scripts, PackageRpath},
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
    gettext: &PackageOutput,
    glibc: &PackageOutput,
    grep: &PackageOutput,
    gzip: &PackageOutput,
    libstdcpp: &PackageOutput,
    linux_headers: &PackageOutput,
    m4: &PackageOutput,
    make: &PackageOutput,
    ncurses: &PackageOutput,
    patch: &PackageOutput,
    patchelf: &PackageOutput,
    sed: &PackageOutput,
    tar: &PackageOutput,
    xz: &PackageOutput,
) -> Result<PackageOutput> {
    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let name = "bison-stage-01";

    let sandbox = PackageSandbox { paths: vec![] };

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        mkdir -pv /bin
        mkdir -pv /lib
        mkdir -pv /lib64
        mkdir -pv /usr/bin

        ln -s ${bash}/bin/bash /bin/bash
        ln -s ${bash}/bin/bash /bin/sh
        ln -s ${gcc}/bin/cpp /lib/cpp
        ln -s ${glibc}/lib/ld-linux-aarch64.so.1 /lib/ld-linux-aarch64.so.1
        ln -s ${glibc}/lib/ld-linux-aarch64.so.1 /lib64/ld-linux-aarch64.so.1

        cd \"${{PWD}}/{source}\"

        ./configure --prefix=\"$output\"

        make -j$({cores})
        make install",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        gcc = gcc.name.to_lowercase().replace("-", "_"),
        glibc = glibc.name.to_lowercase().replace("-", "_"),
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("cb18c2c8562fc01bf3ae17ffe9cf8274e3dd49d39f89397c1a8bac7ee14ce85f".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/bison/bison-3.8.2.tar.xz".to_string(),
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
            gettext.clone(),
            glibc.clone(),
            grep.clone(),
            gzip.clone(),
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
            make.clone(),
            ncurses.clone(),
            patch.clone(),
            patchelf.clone(),
            sed.clone(),
            tar.clone(),
            xz.clone(),
        ],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let glibc_env_key = glibc.name.to_lowercase().replace("-", "_");

    let package_rpaths = vec![PackageRpath {
        rpath: format!("${}/lib", glibc_env_key),
        shrink: true,
        target: "$output".to_string(),
    }];

    let package = add_scripts(package, target, Some(glibc), package_rpaths)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
