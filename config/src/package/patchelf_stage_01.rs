use crate::{
    cross_platform::get_cpu_count,
    sandbox::{
        environments::add_environments,
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
    xz: &PackageOutput,
) -> Result<PackageOutput> {
    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let name = "patchelf";

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
        make install

        export PATH=\"$output/bin:$PATH\"

        mkdir -pv /lib/aarch64-linux-gnu

        ln -s ${gcc}/lib64/libgcc_s.so.1 /lib/aarch64-linux-gnu/libgcc_s.so.1
        ln -s ${glibc}/lib/libc.so.6 /lib/aarch64-linux-gnu/libc.so.6
        ln -s ${glibc}/lib/libm.so.6 /lib/aarch64-linux-gnu/libm.so.6
        ln -s ${libstdcpp}/lib64/libstdc++.so.6 /lib/aarch64-linux-gnu/libstdc++.so.6

        ldd $output/bin/patchelf

        mkdir -pv bin

        cp -v $output/bin/patchelf bin/patchelf

        export PATH=\"${{PWD}}/bin:$PATH\"

        patchelf --version",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        gcc = gcc.name.to_lowercase().replace("-", "_"),
        glibc = glibc.name.to_lowercase().replace("-", "_"),
        libstdcpp = libstdcpp.name.to_lowercase().replace("-", "_"),
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("a278eec544da9f0a82ad7e07b3670cf0f4d85ee13286fa9ad4f4416b700ac19d".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://github.com/NixOS/patchelf/releases/download/0.18.0/patchelf-0.18.0.tar.gz"
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
            m4.clone(),
            make.clone(),
            ncurses.clone(),
            patch.clone(),
            sed.clone(),
            tar.clone(),
            xz.clone(),
        ],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_environments(
        package,
        None,
        None,
        Some(gcc),
        Some(glibc),
        Some(libstdcpp),
        Some(linux_headers),
        None,
    );

    let gcc_env_key = gcc.name.to_lowercase().replace("-", "_");
    let glibc_env_key = glibc.name.to_lowercase().replace("-", "_");

    let package_rpaths = vec![PackageRpath {
        rpath: format!("${}/lib:${}/lib64", glibc_env_key, gcc_env_key),
        shrink: true,
        target: "$output".to_string(),
    }];

    let package = add_scripts(package, target, Some(glibc), package_rpaths)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
