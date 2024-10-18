use crate::{
    cross_platform::get_cpu_count,
    package::{add_default_environment, add_default_script},
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

#[allow(clippy::too_many_arguments)]
pub fn package(
    target: PackageSystem,
    bash: Package,
    binutils: Package,
    coreutils: Package,
    diffutils: Package,
    file: Package,
    findutils: Package,
    gawk: Package,
    gcc: Package,
    glibc: Package,
    grep: Package,
    libstdcpp: Package,
    linux_headers: Package,
    m4: Package,
    ncurses: Package,
    zlib: Package,
) -> Result<Package> {
    let name = "gzip-stage-01";

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        ./configure \
            --build=$(build-aux/config.guess) \
            --prefix=\"$output\"

        make -j$({cores})
        make install",
        bash = bash.name.to_lowercase().replace("-", "_"),
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("25e51d46402bab819045d452ded6c4558ef980f5249c470d9499e9eae34b59b1".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/gzip/gzip-1.13.tar.xz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
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
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
            ncurses.clone(),
            zlib.clone(),
        ],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(
        package,
        Some(bash),
        Some(binutils),
        Some(gcc),
        None,
        Some(libstdcpp),
        Some(linux_headers),
        Some(ncurses),
        Some(zlib),
    );

    let package = add_default_script(package, target, Some(glibc))?;

    Ok(package)
}
