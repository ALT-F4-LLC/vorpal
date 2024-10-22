use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    binutils: &PackageOutput,
    gcc: &PackageOutput,
    linux_headers: &PackageOutput,
    zlib: &PackageOutput,
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

    let package = Package {
        environment: vec![],
        name: name.to_string(),
        packages: vec![
            binutils.clone(),
            gcc.clone(),
            linux_headers.clone(),
            zlib.clone(),
        ],
        sandbox: false,
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
        Some(zlib),
    );

    let package = add_default_script(package, target, None)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
