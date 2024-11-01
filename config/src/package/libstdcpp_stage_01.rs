use crate::{
    cross_platform::get_cpu_count,
    sandbox::{
        environments::add_environments,
        paths::{add_paths, SandboxDefaultPaths},
        scripts::add_scripts,
    },
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    binutils: &PackageOutput,
    gcc: &PackageOutput,
    glibc: &PackageOutput,
    linux_headers: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "libstdcpp-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        mkdir -p build

        cd build

        ../libstdc++-v3/configure \
            --build=$(../config.guess) \
            --disable-libstdcxx-pch \
            --disable-multilib \
            --disable-nls \
            --prefix=\"$output\" \
            --with-gxx-include-dir=\"$output/include/c++/14.2.0\"

        make -j$({cores})
        make install

        rm -v $output/lib64/lib{{stdc++{{,exp,fs}},supc++}}.la",
        cores = get_cpu_count(target)?,
        source = name,
    };

    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let sandbox_paths = SandboxDefaultPaths {
        autoconf: false,
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
        help2man: false,
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
        paths: add_paths(sandbox_paths),
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz".to_string(),
    };

    let package = Package {
        environment,
        name: name.to_string(),
        packages: vec![
            binutils.clone(),
            gcc.clone(),
            glibc.clone(),
            linux_headers.clone(),
        ],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_environments(
        package,
        None,
        Some(binutils),
        Some(gcc),
        None,
        None,
        Some(linux_headers),
        None,
    );

    let package = add_scripts(package, target, None, vec![])?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
