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

pub fn package(context: &mut ContextConfig, target: PackageSystem) -> Result<PackageOutput> {
    let name = "binutils-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        mkdir -p \"${{PWD}}/{source}\"/build
        cd \"${{PWD}}/{source}\"/build

        ../configure \
            --disable-nls \
            --disable-werror \
            --enable-default-hash-style=\"gnu\" \
            --enable-gprofng=\"no\" \
            --enable-new-dtags \
            --prefix=\"$output\"

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count(target)?
    };

    let environment = vec![
        PackageEnvironment {
            key: "CC".to_string(),
            value: "/usr/bin/gcc".to_string(),
        },
        PackageEnvironment {
            key: "GCC".to_string(),
            value: "/usr/bin/gcc".to_string(),
        },
        PackageEnvironment {
            key: "PATH".to_string(),
            value: "/usr/lib/gcc/aarch64-linux-gnu/12:/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
        },
    ];

    let source = PackageSource {
        excludes: vec![],
        hash: Some("c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz".to_string(),
    };

    let sandbox_paths = SandboxDefaultPaths {
        autoconf: false,
        automake: true,
        bash: true,
        binutils: true,
        bison: true,
        bzip2: true,
        coreutils: true,
        curl: true,
        diffutils: true,
        file: true,
        findutils: true,
        flex: true,
        gawk: true,
        gcc: true,
        gcc_12: true,
        glibc: true,
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

    let package = Package {
        environment,
        name: name.to_string(),
        packages: vec![],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_environments(package, None, None, None, None, None, None, None);

    let package = add_scripts(package, target, None, vec![])?;

    let package_input = context.add_package(package)?;

    Ok(package_input)
}
