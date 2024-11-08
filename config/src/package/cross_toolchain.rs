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

            export CPPFLAGS=\"-I$output/usr/include -I$output/usr/include/c++/14 -I/usr/include\"
            export C_INCLUDE_PATH=\"$output/usr/include:$output/usr/include/c++/14:/usr/include\"

            export LDFLAGS=\"-L$output/lib -L/lib -L/lib64\"
            export LD_LIBRARY_PATH=\"$output/lib:/lib:/lib64\"
            export LIBRARY_PATH=\"$output/lib:/lib:/lib64\"

            ### Set local values

            target=\"$(uname -m)-vorpal-linux-gnu\"

            ### Build binutils (stage 01)

            mkdir -pv ./binutils/build-01
            pushd ./binutils/build-01

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

            ### Build gcc (stage 01)

            pushd ./gcc

            ./contrib/download_prerequisites

            case $(uname -m) in
              x86_64)
                sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
             ;;
            esac

            mkdir -pv ./build-gcc-01
            pushd ./build-gcc-01

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

            rm -v $output/usr/lib/lib{{stdc++{{,exp,fs}},supc++}}.la

            popd
            popd

            ## Build m4

            pushd ./m4

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build ncurses

            pushd ./ncurses

            sed -i s/mawk// configure

            mkdir -pv ./build
            pushd ./build

            ../configure

            make -C include
            make -C progs tic

            popd

            ./configure \
                --build=\"$(./config.guess)\" \
                --disable-stripping \
                --host=\"$target\" \
                --mandir=\"/usr/share/man\" \
                --prefix=\"/usr\" \
                --with-cxx-shared \
                --with-manpage-format=\"normal\" \
                --with-shared \
                --without-ada \
                --without-debug \
                --without-normal

            make
            make DESTDIR=\"$output\" TIC_PATH=\"$(pwd)/build/progs/tic\" install

            ln -sv libncursesw.so $output/usr/lib/libncurses.so
            sed -e 's/^#if.*XOPEN.*$/#if 1/' -i $output/usr/include/curses.h

            popd

            ## Build bash

            pushd ./bash

            ./configure \
                --build=\"$(sh support/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\" \
                --without-bash-malloc \
                bash_cv_strtold_broken=\"no\"

            make
            make DESTDIR=\"$output\" install

            ln -sv bash $output/usr/bin/sh

            popd

            ## Build coreutils

            pushd ./coreutils

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --enable-install-program=\"hostname\" \
                --enable-no-install-program=\"kill,uptime\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            mv -v $output/usr/bin/chroot $output/usr/sbin
            mkdir -pv $output/usr/share/man/man8
            mv -v $output/usr/share/man/man1/chroot.1 $output/usr/share/man/man8/chroot.8
            sed -i 's/\"1\"/\"8\"/' $output/usr/share/man/man8/chroot.8

            popd

            ## Build diffutils

            pushd ./diffutils

            ./configure \
                --build=\"$(./build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build file

            pushd ./file

            mkdir -pv ./build

            pushd ./build

            ../configure \
                --disable-bzlib \
                --disable-libseccomp \
                --disable-xzlib \
                --disable-zlib

            make

            popd

            ./configure \
                --build=\"$(./config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make FILE_COMPILE=$(pwd)/build/src/file
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/libmagic.la

            popd

            ## Build findutils

            pushd ./findutils

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --host=\"$target\" \
                --localstatedir=\"/var/lib/locate\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build gawk

            pushd ./gawk

            sed -i 's/extras//' Makefile.in

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build grep

            pushd ./grep

            ./configure \
                --build=\"$(./build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build gzip

            pushd ./gzip

            ./configure \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build make

            pushd ./make

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\" \
                --without-guile

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build patch

            pushd ./patch

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build sed

            pushd ./sed

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build tar

            pushd ./tar

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            popd

            ## Build xz

            pushd ./xz

            ./configure \
                --build=\"$(build-aux/config.guess)\" \
                --disable-static \
                --docdir=\"/usr/share/doc/xz-5.6.2\" \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/liblzma.la

            popd

            ## Build binutils (stage 02)

            pushd ./binutils

            sed '6009s/$add_dir//' -i ltmain.sh

            mkdir -pv ./build-02
            pushd ./build-02

            ../configure \
                --build=\"$(../config.guess)\" \
                --disable-nls \
                --disable-werror \
                --enable-64-bit-bfd \
                --enable-default-hash-style=\"gnu\" \
                --enable-gprofng=\"no\" \
                --enable-new-dtags \
                --enable-shared \
                --host=\"$target\" \
                --prefix=\"/usr\"

            make
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/lib{{bfd,ctf,ctf-nobfd,opcodes,sframe}}.{{a,la}}

            popd
            popd

            ## Build gcc (stage 02)

            pushd ./gcc

            sed '/thread_header =/s/@.*@/gthr-posix.h/' \
                -i libgcc/Makefile.in libstdc++-v3/include/Makefile.in

            mkdir -pv ./build-gcc-02
            pushd ./build-gcc-02

            ../configure \
                --build=\"$(../config.guess)\" \
                --disable-libatomic \
                --disable-libgomp \
                --disable-libquadmath \
                --disable-libsanitizer \
                --disable-libssp \
                --disable-libvtv \
                --disable-multilib \
                --disable-nls \
                --enable-default-pie \
                --enable-default-ssp \
                --enable-languages=\"c,c++\" \
                --host=\"$target\" \
                --prefix=\"/usr\" \
                --target=\"$target\" \
                --with-build-sysroot=\"$output\" \
                LDFLAGS_FOR_TARGET=\"-L$PWD/$target/libgcc\"

            make
            make DESTDIR=\"$output\" install

            ln -sv gcc $output/usr/bin/cc",
            target_host = "x86_64-linux-gnu",
        },
        // TODO: explore making docker image a source
        source: vec![
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662".to_string(),
                ),
                includes: vec![],
                name: "bash".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/bash/bash-5.2.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528".to_string(),
                ),
                includes: vec![],
                name: "binutils".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4".to_string(),
                ),
                includes: vec![],
                name: "coreutils".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "5045e29e7fa0ffe017f63da7741c800cbc0f89e04aebd78efcd661d6e5673326".to_string(),
                ),
                includes: vec![],
                name: "diffutils".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/diffutils/diffutils-3.10.tar.xz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "c118ab56efa05798022a5a488827594a82d844f65159e95b918d5501adf1e58f".to_string(),
                ),
                includes: vec![],
                name: "file".to_string(),
                strip_prefix: true,
                uri: "https://astron.com/pub/file/file-5.45.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "242f804d87a5036bb0fab99966227dc61e853e5a67e1b10c3cc45681c792657e".to_string(),
                ),
                includes: vec![],
                name: "findutils".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/findutils/findutils-4.10.0.tar.xz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "f82947e3d4fed9bec5ec686b4a511d6720a23eb809f41b1dbcee30a347f9cb7b".to_string(),
                ),
                includes: vec![],
                name: "gawk".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/gawk/gawk-5.3.1.tar.xz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583".to_string(),
                ),
                includes: vec![],
                name: "gcc".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b".to_string(),
                ),
                includes: vec![],
                name: "glibc".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/glibc/glibc-2.40.tar.gz".to_string(),
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
                    "1625eae01f6e4dbc41b58545aa2326c74791b2010434f8241d41903a4ea5ff70".to_string(),
                ),
                includes: vec![],
                name: "grep".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/grep/grep-3.11.tar.xz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "25e51d46402bab819045d452ded6c4558ef980f5249c470d9499e9eae34b59b1".to_string(),
                ),
                includes: vec![],
                name: "gzip".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/gzip/gzip-1.13.tar.xz".to_string(),
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
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "fd793cdfc421fac76f4af23c7d960cbe4a29cbb18f5badf37b85e16a894b3b6d".to_string(),
                ),
                includes: vec![],
                name: "m4".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/m4/m4-1.4.19.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "8dfe7b0e51b3e190cd75e046880855ac1be76cf36961e5cfcc82bfa91b2c3ba8".to_string(),
                ),
                includes: vec![],
                name: "make".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/make/make-4.4.1.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "aab234a3b7a22e2632151fbe550cb36e371d3ee5318a633ee43af057f9f112fb".to_string(),
                ),
                includes: vec![],
                name: "ncurses".to_string(),
                strip_prefix: true,
                uri: "https://invisible-mirror.net/archives/ncurses/ncurses-6.5.tar.gz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "af8c281a05a6802075799c0c179e5fb3a218be6a21b726d8b672cd0f4c37eae9".to_string(),
                ),
                includes: vec![],
                name: "patch".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/patch/patch-2.7.6.tar.xz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "434ff552af89340088e0d8cb206c251761297909bbee401176bc8f655e8e7cf2".to_string(),
                ),
                includes: vec![],
                name: "sed".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/sed/sed-4.9.tar.xz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "f9bb5f39ed45b1c6a324470515d2ef73e74422c5f345503106d861576d3f02f3".to_string(),
                ),
                includes: vec![],
                name: "tar".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/tar/tar-1.35.tar.xz".to_string(),
            },
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "7a02b1278ed9a59b332657d613c5549b39afe34e315197f4da95c5322524ec26".to_string(),
                ),
                includes: vec![],
                name: "xz".to_string(),
                strip_prefix: true,
                uri:
                    "https://github.com/tukaani-project/xz/releases/download/v5.6.2/xz-5.6.2.tar.xz"
                        .to_string(),
            },
        ],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    context.add_package(package)
}
