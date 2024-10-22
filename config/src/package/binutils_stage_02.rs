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
    _binutils: &PackageOutput,
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
    let name = "binutils-stage-02";

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        sed '6009s/$add_dir//' -i ltmain.sh

        mkdir -v build

        cd build

        ../configure \
            --build=$(../config.guess) \
            --disable-nls \
            --disable-werror \
            --enable-64-bit-bfd \
            --enable-default-hash-style=\"gnu\" \
            --enable-gprofng=\"no\" \
            --enable-new-dtags \
            --enable-shared \
            --prefix=\"$output\"

        make -j$({cores})
        make install

        rm -v $output/lib/lib{{bfd,ctf,ctf-nobfd,opcodes,sframe}}.{{a,la}}",
        bash = bash.name.to_lowercase().replace("-", "_"),
        source = name,
        cores = get_cpu_count(target)?
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz".to_string(),
    };

    let package = Package {
        environment: vec![],
        name: name.to_string(),
        packages: vec![
            bash.clone(),
            // binutils.clone(),
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
            make.clone(),
            m4.clone(),
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
        None,
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
