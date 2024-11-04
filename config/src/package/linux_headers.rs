use crate::{
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
) -> Result<PackageOutput> {
    let name = "linux-headers";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        make mrproper
        make headers

        find usr/include -type f ! -name '*.h' -delete

        mkdir -p \"$output/usr\"
        cp -rv usr/include \"$output/usr\"",
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("3fa3f4f3d010de5b9bde09d08a251fa3ef578d356d3a7a29b6784a6916ea0d50".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.10.8.tar.xz".to_string(),
    };

    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let sandbox_paths = SandboxDefaultPaths {
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
        packages: vec![binutils.clone(), gcc.clone()],
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
        None,
        None,
    );

    let package = add_scripts(package, target, None, vec![])?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
