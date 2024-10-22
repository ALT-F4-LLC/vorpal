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

#[allow(clippy::too_many_arguments)]
pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    bash: &PackageOutput,
    binutils: &PackageOutput,
    coreutils: &PackageOutput,
    file: &PackageOutput,
    findutils: &PackageOutput,
    gcc: &PackageOutput,
    glibc: &PackageOutput,
    libstdcpp: &PackageOutput,
    linux_headers: &PackageOutput,
    m4: &PackageOutput,
    ncurses: &PackageOutput,
    zlib: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "gawk-stage-01";

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        sed -i 's/extras//' Makefile.in

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
        hash: Some("f82947e3d4fed9bec5ec686b4a511d6720a23eb809f41b1dbcee30a347f9cb7b".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/gawk/gawk-5.3.1.tar.xz".to_string(),
    };

    let package = Package {
        environment: vec![],
        name: name.to_string(),
        packages: vec![
            bash.clone(),
            binutils.clone(),
            coreutils.clone(),
            file.clone(),
            findutils.clone(),
            gcc.clone(),
            glibc.clone(),
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
            ncurses.clone(),
            zlib.clone(),
        ],
        sandbox: false,
        script,
        source: vec![source],
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

    let package_input = context.add_package(package)?;

    Ok(package_input)
}
