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
    zlib: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "gettext-stage-01";

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        ./configure --disable-shared --prefix=\"$output\"

        make -j$({cores})
        make install",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("6e3ef842d1006a6af7778a8549a8e8048fc3b923e5cf48eaa5b82b5d142220ae".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/gettext/gettext-0.22.5.tar.xz".to_string(),
    };

    let package = Package {
        environment: vec![],
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

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
