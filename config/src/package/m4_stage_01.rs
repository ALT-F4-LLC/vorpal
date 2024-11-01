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
    binutils: &PackageOutput,
    gcc: &PackageOutput,
    glibc: &PackageOutput,
    libstdcpp: &PackageOutput,
    linux_headers: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "m4-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        ./configure \
            --build=$(build-aux/config.guess) \
            --prefix=$output

        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count(target)?,
    };

    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

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
        hash: Some("fd793cdfc421fac76f4af23c7d960cbe4a29cbb18f5badf37b85e16a894b3b6d".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/m4/m4-1.4.19.tar.gz".to_string(),
    };

    let package = Package {
        environment,
        name: name.to_string(),
        packages: vec![
            binutils.clone(),
            gcc.clone(),
            glibc.clone(),
            libstdcpp.clone(),
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
        Some(libstdcpp),
        Some(linux_headers),
        None,
    );

    let packages_rpaths = vec![PackageRpath {
        rpath: format!("${}/lib", glibc.name.to_lowercase().replace("-", "_")),
        shrink: true,
        target: "$output".to_string(),
    }];

    let package = add_scripts(package, target, Some(glibc), packages_rpaths)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
