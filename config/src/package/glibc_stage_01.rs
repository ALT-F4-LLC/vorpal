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

pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    binutils: &PackageOutput,
    gcc: &PackageOutput,
    linux_headers: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "glibc-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        mkdir -p \"${{PWD}}/{source}/build\"
        cd \"${{PWD}}/{source}/build\"

        echo \"rootsbindir=$output/sbin\" > configparms

        ../configure \
            --build=$(../scripts/config.guess) \
            --disable-nscd \
            --prefix=\"$output\" \
            --with-binutils=\"${binutils}/bin\" \
            --with-headers=\"${linux_headers}/usr/include\" \
            libc_cv_slibdir=\"$output/lib\"

        make -j$({cores})
        make install",
        binutils = binutils.name.to_lowercase().replace("-", "_"),
        linux_headers = linux_headers.name.to_lowercase().replace("-", "_"),
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/glibc/glibc-2.40.tar.gz".to_string(),
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
        glibc: true,
        grep: true,
        gzip: true,
        help2man: false,
        includes: true,
        lib: true,
        m4: true,
        make: true,
        patchelf: false,
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
        packages: vec![binutils.clone(), gcc.clone(), linux_headers.clone()],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(
        package,
        None,
        Some(binutils),
        Some(gcc),
        None,
        None,
        Some(linux_headers),
        None,
        None,
    );

    let package = add_default_script(package, target, None)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
