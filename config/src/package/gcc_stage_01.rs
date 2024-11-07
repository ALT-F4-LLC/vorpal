use crate::{
    sandbox::{environments, paths},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSandbox, PackageSource,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(context: &mut ContextConfig, binutils: &PackageOutput) -> Result<PackageOutput> {
    let name = "gcc-stage-01";

    let package = Package {
        environment: environments::add_rootfs(context.get_target())?,
        name: name.to_string(),
        packages: vec![binutils.clone()],
        sandbox: Some(PackageSandbox {
            paths: paths::add_rootfs(),
        }),
        script: formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            cd {source}

            ./contrib/download_prerequisites

            case $(uname -m) in
              x86_64)
                sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
             ;;
            esac

            mkdir -pv build

            cd build

            ../configure \
                --disable-libatomic \
                --disable-libgomp \
                --disable-libquadmath \
                --disable-libssp \
                --disable-libstdcxx \
                --disable-libvtv \
                --disable-multilib \
                --disable-nls \
                --disable-shared \
                --disable-threads \
                --enable-default-pie \
                --enable-default-ssp \
                --enable-languages=\"c,c++\" \
                --prefix=\"$output\" \
                --with-glibc-version=\"2.40\" \
                --with-ld=\"${binutils}/bin/ld\" \
                --with-newlib \
                --with-sysroot=\"$output\" \
                --without-headers

            make -j$(nproc)
            make install

            cd ..

            OUTPUT_LIBGCC=$(cd $output && bin/{target}-gcc -print-libgcc-file-name)
            OUTPUT_LIBGCC_DIR=$(dirname \"${{OUTPUT_LIBGCC}}\")

            cat gcc/limitx.h gcc/glimits.h gcc/limity.h > \
                ${{OUTPUT_LIBGCC_DIR}}/include/limits.h

            cp $output/bin/gcc $output/bin/cc",
            binutils = binutils.name.to_lowercase().replace("-", "_"),
            source = name,
            target = "aarch64-unknown-linux-gnu",
        },
        source: vec![PackageSource {
            excludes: vec![],
            hash: Some(
                "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583".to_string(),
            ),
            includes: vec![],
            name: name.to_string(),
            strip_prefix: true,
            uri: "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz".to_string(),
        }],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    context.add_package(package)
}
