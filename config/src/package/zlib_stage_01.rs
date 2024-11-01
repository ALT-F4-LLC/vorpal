use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script, PackageEnvironment},
    sandbox::{add_default_host_paths, SandboxDefaultPaths},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSandbox, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(context: &mut ContextConfig, target: PackageSystem) -> Result<PackageOutput> {
    let name = "zlib-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        ./configure --prefix=$output

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count(target)?,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("3f7995d5f103719283f509c23624287ce95c349439e881ed935a3c2c807bb683".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz"
            .to_string(),
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
        flex: false,
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
        paths: add_default_host_paths(sandbox_paths),
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

    let package = add_default_environment(package, None, None, None, None, None, None, None);

    let package = add_default_script(package, target, None)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
