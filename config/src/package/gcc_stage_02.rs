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

pub fn package(target: PackageSystem, binutils: Package, zlib: Package) -> Result<Package> {
    let name = "gcc-stage-01";

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        cd ${{PWD}}/{source}

        ./contrib/download_prerequisites

        case $(uname -m) in
          x86_64)
            sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
         ;;
        esac

        mkdir -p build

        cd build

        ../configure \
            --disable-libatomic \
            --disable-libcc1 \
            --disable-libgomp \
            --disable-libquadmath \
            --disable-libssp \
            --disable-libvtv \
            --disable-multilib \
            --disable-nls \
            --disable-threads \
            --enable-default-pie \
            --enable-default-ssp \
            --enable-languages=\"c,c++\" \
            --prefix=\"$output\" \
            --with-ld=\"${binutils}/bin/ld\" \
            --with-newlib \
            --without-headers

        make -j$({cores})
        make install

        cd ..

        OUTPUT_LIBGCC=$(cd $output && bin/{target}-gcc -print-libgcc-file-name)
        OUTPUT_LIBGCC_DIR=$(dirname \"${{OUTPUT_LIBGCC}}\")

        cat gcc/limitx.h gcc/glimits.h gcc/limity.h > \
            ${{OUTPUT_LIBGCC_DIR}}/include/limits.h

        cp $output/bin/gcc $output/bin/cc",
        binutils = binutils.name.to_lowercase().replace("-", "_"),
        source = name,
        cores = get_cpu_count(target)?,
        target = "aarch64-unknown-linux-gnu",
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![binutils.clone(), zlib.clone()],
        sandbox: false,
        script,
        source: HashMap::from([(name.to_string(), source)]),
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_default_environment(
        package,
        None,
        Some(binutils),
        None,
        None,
        None,
        None,
        None,
        Some(zlib),
    );

    let package = add_default_script(package, target, None)?;

    Ok(package)
}
