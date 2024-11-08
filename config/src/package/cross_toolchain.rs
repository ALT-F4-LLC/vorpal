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

pub fn package(context: &mut ContextConfig) -> Result<PackageOutput> {
    let name = "cross-toolchain";

    let package = Package {
        environment: environments::add_rootfs()?,
        name: name.to_string(),
        packages: vec![],
        sandbox: Some(PackageSandbox {
            paths: paths::add_rootfs(),
        }),
        script: formatdoc! {"
            #!/bin/bash
            set -euo +h pipefail
            umask 022

            ### Set global values

            export CONFIG_SITE=\"$output/usr/share/config.site\"
            export LC_ALL=\"POSIX\"
            export MAKEFLAGS=\"-j$(nproc)\"
            export PATH=\"$output/bin:$PATH\"

            ### Set build values

            export CXX=\"/usr/bin/{target_host}-g++\"
            export GCC=\"/usr/bin/{target_host}-gcc\"
            export CC=\"$GCC\"

            export CXX_FOR_TARGET=\"$CXX\"
            export GCC_FOR_TARGET=\"$GCC\"
            export CC_FOR_TARGET=\"$CC\"

            export CPPFLAGS=\"-I$output/usr/include -I/usr/include\"
            export C_INCLUDE_PATH=\"$output/usr/include:/usr/include\"

            export LDFLAGS=\"-L$output/lib -L/lib -L/lib64\"
            export LD_LIBRARY_PATH=\"$output/lib:/lib:/lib64\"
            export LIBRARY_PATH=\"$output/lib:/lib:/lib64\"

            ### Set local values

            target=\"$(uname -m)-vorpal-linux-gnu\"

            ### Build binutils

            mkdir -pv ./binutils/build
            pushd ./binutils/build

            ../configure \
                --disable-nls \
                --disable-werror \
                --enable-default-hash-style=\"gnu\" \
                --enable-gprofng=\"no\" \
                --enable-new-dtags \
                --prefix=\"$output\" \
                --target=\"$target\" \
                --with-sysroot=\"$output\"

            make
            make install

            popd

            ### Build gcc

            pushd ./gcc

            ./contrib/download_prerequisites

            case $(uname -m) in
              x86_64)
                sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
             ;;
            esac

            mkdir -pv ./build-gcc
            pushd ./build-gcc

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
                --target=\"$target\" \
                --with-glibc-version=\"2.40\" \
                --with-ld=\"$output/$target/bin/ld\" \
                --with-newlib \
                --with-sysroot=\"$output\" \
                --without-headers

            make
            make install

            popd

            OUTPUT_LIBGCC=$(cd $output && bin/$target-gcc -print-libgcc-file-name)
            OUTPUT_LIBGCC_DIR=$(dirname \"${{OUTPUT_LIBGCC}}\")

            cat gcc/limitx.h gcc/glimits.h gcc/limity.h > \
                ${{OUTPUT_LIBGCC_DIR}}/include/limits.h

            # TODO: see if we can remove this
            # cp -v $output/bin/$target-gcc $output/bin/$target-cc

            popd

            ### Update build values for new gcc

            export CXX=\"$output/bin/$target-g++\"
            export GCC=\"$output/bin/$target-gcc\"
            export CC=\"$GCC\"

            export CXX_FOR_TARGET=\"$CXX\"
            export GCC_FOR_TARGET=\"$GCC\"
            export CC_FOR_TARGET=\"$CC\"

            ### Build linux headers

            pushd ./linux-headers

            make mrproper
            make headers

            find usr/include -type f ! -name '*.h' -delete

            mkdir -p \"$output/usr\"
            cp -rv usr/include \"$output/usr\"

            popd

            ### Build glibc

            case $(uname -m) in
                i?86)   ln -sfv ld-linux.so.2 $output/lib/ld-lsb.so.3
                ;;
                x86_64) mkdir -pv $output/lib64
                        ln -sfv /lib/x86_64-linux-gnu/ld-linux-x86-64.so.2 $output/lib64/ld-linux-x86-64.so.2
                        ln -sfv /lib/x86_64-linux-gnu/ld-linux-x86-64.so.2 $output/lib64/ld-lsb-x86-64.so.3
                ;;
            esac

            pushd ./glibc

            patch -Np1 -i ../glibc-patch/glibc-2.40-fhs-1.patch

            mkdir -pv ./build
            pushd ./build

            echo \"rootsbindir=/usr/sbin\" > configparms

            ../configure \
                --build=$(../scripts/config.guess) \
                --disable-nscd \
                --enable-kernel=\"4.19\" \
                --host=\"$target\" \
                --prefix=\"/usr\" \
                --with-headers=\"$output/usr/include\" \
                libc_cv_slibdir=\"/usr/lib\"

            make
            make DESTDIR=$output install

            # TODO: this needs to be prefix with $output

            sed '/RTLDLIST=/s@/usr@@g' -i $output/usr/bin/ldd

            popd
            popd

            ## Replace linux symlinks

            rm -v $output/lib64/ld-linux-x86-64.so.2
            rm -v $output/lib64/ld-lsb-x86-64.so.3

            ln -sfv $output/usr/lib/ld-linux-x86-64.so.2 $output/lib64/ld-linux-x86-64.so.2
            ln -sfv $output/usr/lib/ld-linux-x86-64.so.2 $output/lib64/ld-lsb-x86-64.so.3

            ## Test glibc

            echo 'Testing glibc'

            echo 'int main(){{}}' | $output/bin/$target-gcc -xc -

            ls -alh

            readelf -l a.out | grep ld-linux

            rm -v a.out

            ## Build libstdc++

            pushd ./gcc

            mkdir -pv ./build-libstdc++
            pushd ./build-libstdc++

            ../libstdc++-v3/configure \
                --build=$(../config.guess) \
                --disable-libstdcxx-pch \
                --disable-multilib \
                --disable-nls \
                --host=\"$target\" \
                --prefix=\"/usr\" \
                --with-gxx-include-dir=/usr/include/c++/14

            make
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/lib{{stdc++{{,exp,fs}},supc++}}.la",
            target_host = "x86_64-linux-gnu",
        },
        // TODO: explore making docker image a source
        source: vec![
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528".to_string(),
                ),
                includes: vec![],
                name: "binutils".to_string(),
                strip_prefix: true,
                uri: "https://ftp.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583".to_string(),
                ),
                includes: vec![],
                name: "gcc".to_string(),
                strip_prefix: true,
                uri: "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b".to_string(),
                ),
                includes: vec![],
                name: "glibc".to_string(),
                strip_prefix: true,
                uri: "https://ftp.gnu.org/gnu/glibc/glibc-2.40.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "69cf0653ad0a6a178366d291f30629d4e1cb633178aa4b8efbea0c851fb944ca".to_string(),
                ),
                includes: vec![],
                name: "glibc-patch".to_string(),
                strip_prefix: false,
                uri: "https://www.linuxfromscratch.org/patches/lfs/12.2/glibc-2.40-fhs-1.patch"
                    .to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "3fa3f4f3d010de5b9bde09d08a251fa3ef578d356d3a7a29b6784a6916ea0d50".to_string(),
                ),
                includes: vec![],
                name: "linux-headers".to_string(),
                strip_prefix: true,
                uri: "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.10.8.tar.xz".to_string(),
            },
        ],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    context.add_package(package)
}
