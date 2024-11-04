use crate::{
    cross_platform::get_cpu_count,
    sandbox::{
        environments::add_environments,
        paths::{add_paths, SandboxDefaultPaths},
        scripts::add_scripts,
    },
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    binutils: &PackageOutput,
) -> Result<PackageOutput> {
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
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz".to_string(),
    };

    let environment = vec![
        PackageEnvironment {
            key: "CC".to_string(),
            value: "/usr/bin/gcc".to_string(),
        },
        PackageEnvironment {
            key: "GCC".to_string(),
            value: "/usr/bin/gcc".to_string(),
        },
        PackageEnvironment {
            key: "PATH".to_string(),
            value: "/usr/lib/gcc/aarch64-linux-gnu/12:/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
        },
    ];

    let sandbox_paths = SandboxDefaultPaths {
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
        gcc: true,
        gcc_12: true,
        glibc: true,
        grep: true,
        gzip: true,
        help2man: false,
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

    let package = Package {
        environment,
        name: name.to_string(),
        packages: vec![binutils.clone()],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let package = add_environments(package, None, Some(binutils), None, None, None, None, None);

    let package = add_scripts(package, target, None, vec![])?;

    let package_input = context.add_package(package)?;

    Ok(package_input)
}
