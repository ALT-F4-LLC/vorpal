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
    bison: &PackageOutput,
    coreutils: &PackageOutput,
    diffutils: &PackageOutput,
    file: &PackageOutput,
    findutils: &PackageOutput,
    gawk: &PackageOutput,
    gcc: &PackageOutput,
    gettext: &PackageOutput,
    glibc: &PackageOutput,
    grep: &PackageOutput,
    gzip: &PackageOutput,
    libstdcpp: &PackageOutput,
    linux_headers: &PackageOutput,
    m4: &PackageOutput,
    make: &PackageOutput,
    ncurses: &PackageOutput,
    patch: &PackageOutput,
    perl: &PackageOutput,
    python: &PackageOutput,
    sed: &PackageOutput,
    tar: &PackageOutput,
    texinfo: &PackageOutput,
    xz: &PackageOutput,
    zlib: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "util-linux-stage-01";

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        cd \"${{PWD}}/{source}\"

        mkdir -pv $output/var/lib/hwclock

        ADJTIME_PATH=$output/var/lib/hwclock/adjtime \
        ./configure \
            --disable-chfn-chsh \
            --disable-liblastlog2 \
            --disable-login \
            --disable-nologin \
            --disable-pylibmount \
            --disable-runuser \
            --disable-setpriv \
            --disable-static \
            --disable-su \
            --runstatedir=/run \
            --without-python

        make -j$({cores})
        make install",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://www.kernel.org/pub/linux/utils/util-linux/v2.40/util-linux-2.40.2.tar.xz"
            .to_string(),
    };

    let package = Package {
        environment: vec![],
        name: name.to_string(),
        packages: vec![
            bash.clone(),
            binutils.clone(),
            bison.clone(),
            coreutils.clone(),
            diffutils.clone(),
            file.clone(),
            findutils.clone(),
            gawk.clone(),
            gcc.clone(),
            gettext.clone(),
            glibc.clone(),
            grep.clone(),
            gzip.clone(),
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
            make.clone(),
            ncurses.clone(),
            patch.clone(),
            perl.clone(),
            python.clone(),
            sed.clone(),
            tar.clone(),
            texinfo.clone(),
            xz.clone(),
            zlib.clone(),
        ],
        sandbox: None,
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
