use crate::ContextConfig;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSandboxPath, PackageSource,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(context: &mut ContextConfig) -> Result<PackageOutput> {
    let name = "cross-toolchain";

    // TODO: explore making exported image a source

    let sandbox_rootfs_path = "/vorpal/sandbox-rootfs";

    let package = Package {
        // TODO: explore moving environment into sandbox
        environment: vec![PackageEnvironment {
            key: "PATH".to_string(),
            value: "/usr/bin".to_string(),
        }],
        name: name.to_string(),
        packages: vec![],
        sandbox: Some(PackageSandbox {
            paths: vec![
                PackageSandboxPath {
                    source: format!("{}/bin", sandbox_rootfs_path),
                    symlink: false,
                    target: "/bin".to_string(),
                },
                PackageSandboxPath {
                    source: format!("{}/etc", sandbox_rootfs_path),
                    symlink: false,
                    target: "/etc".to_string(),
                },
                PackageSandboxPath {
                    source: format!("{}/lib", sandbox_rootfs_path),
                    symlink: false,
                    target: "/lib".to_string(),
                },
                PackageSandboxPath {
                    source: format!("{}/usr/lib/x86_64-linux-gnu", sandbox_rootfs_path),
                    symlink: false,
                    target: "/lib64".to_string(),
                },
                PackageSandboxPath {
                    source: format!("{}/usr", sandbox_rootfs_path),
                    symlink: false,
                    target: "/usr".to_string(),
                },
                PackageSandboxPath {
                    source: format!("{}/sbin", sandbox_rootfs_path),
                    symlink: false,
                    target: "/sbin".to_string(),
                },
                PackageSandboxPath {
                    source: format!("{}/var", sandbox_rootfs_path),
                    symlink: false,
                    target: "/var".to_string(),
                },
            ],
        }),
        script: formatdoc! {"
            #!/bin/bash
            set -euo +h pipefail
            umask 022

            ## Setup paths

            mkdir -pv $output/{{etc,var}} $output/usr/{{bin,lib,sbin}}

            for i in bin lib sbin; do
              ln -sv usr/$i $output/$i
            done

            case $(uname -m) in
              x86_64) mkdir -pv $output/lib64 ;;
            esac

            mkdir -pv $output/tools

            ### Setup environment

            export LC_ALL=\"POSIX\"
            export TARGET=\"$(uname -m)-vorpal-linux-gnu\"
            export PATH=\"$output/tools/bin:$PATH\"
            export CONFIG_SITE=\"$output/usr/share/config.site\"
            export MAKEFLAGS=\"-j$(nproc)\"

            ### Setup duplicate sources

            mkdir -pv libstdc++
            mkdir -pv binutils-pass-02
            mkdir -pv gcc-pass-02

            rsync -av gcc/ libstdc++/
            rsync -av binutils/ binutils-pass-02/
            rsync -av gcc/ gcc-pass-02/

            mv -v binutils binutils-pass-01
            mv -v gcc gcc-pass-01

            ### Build binutils (pass 01)

            pushd ./binutils-pass-01

            mkdir -pv ./build

            pushd ./build

            ../configure \
                --prefix=\"$output/tools\" \
                --with-sysroot=\"$output\" \
                --target=\"$TARGET\" \
                --disable-nls \
                --enable-gprofng=\"no\" \
                --disable-werror \
                --enable-new-dtags \
                --enable-default-hash-style=\"gnu\"

            make
            make install

            popd
            popd

            rm -rf ./binutils-pass-01

            ### Build gcc (pass 01)

            pushd ./gcc-pass-01

            ./contrib/download_prerequisites

            case $(uname -m) in
              x86_64)
                sed -e '/m64=/s/lib64/lib/' \
                    -i.orig gcc/config/i386/t-linux64
             ;;
            esac

            mkdir -pv ./build

            pushd ./build

            ../configure \
                --target=\"$TARGET\" \
                --prefix=\"$output/tools\" \
                --with-glibc-version=\"2.40\" \
                --with-sysroot=\"$output\" \
                --with-newlib \
                --without-headers \
                --enable-default-pie \
                --enable-default-ssp \
                --disable-nls \
                --disable-shared \
                --disable-multilib \
                --disable-threads \
                --disable-libatomic \
                --disable-libgomp \
                --disable-libquadmath \
                --disable-libssp \
                --disable-libvtv \
                --disable-libstdcxx \
                --enable-languages=\"c,c++\"

            make
            make install

            popd

            OUTPUT_LIBGCC=$($TARGET-gcc -print-libgcc-file-name)
            OUTPUT_LIBGCC_DIR=$(dirname \"${{OUTPUT_LIBGCC}}\")
            OUTPUT_LIMITS_PATH=${{OUTPUT_LIBGCC_DIR}}/include/limits.h

            echo \"OUTPUT_LIBGCC: ${{OUTPUT_LIBGCC}}\"
            echo \"OUTPUT_LIBGCC_DIR: ${{OUTPUT_LIBGCC_DIR}}\"
            echo \"OUTPUT_LIMITS_PATH: ${{OUTPUT_LIMITS_PATH}}\"

            cat gcc/limitx.h gcc/glimits.h gcc/limity.h > $OUTPUT_LIMITS_PATH

            popd

            rm -rf ./gcc-pass-01

            ### Build linux headers

            pushd ./linux-headers

            make mrproper
            make headers

            find usr/include -type f ! -name '*.h' -delete

            cp -rv usr/include \"$output/usr\"

            popd

            rm -rf ./linux-headers

            ### Build glibc

            pushd ./glibc

            case $(uname -m) in
                i?86)   ln -sfv ld-linux.so.2 $output/lib/ld-lsb.so.3
                ;;
                x86_64) ln -sfv ../lib/ld-linux-x86-64.so.2 $output/lib64
                        ln -sfv ../lib/ld-linux-x86-64.so.2 $output/lib64/ld-lsb-x86-64.so.3
                ;;
            esac

            ls -alh $output/lib64

            patch -Np1 -i ../glibc-patch/glibc-2.40-fhs-1.patch

            mkdir -pv ./build

            pushd ./build

            echo \"rootsbindir=/usr/sbin\" > configparms

            ../configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(../scripts/config.guess)\" \
                --enable-kernel=\"4.19\" \
                --with-headers=\"$output/usr/include\" \
                --disable-nscd \
                libc_cv_slibdir=\"/usr/lib\"

            make
            make DESTDIR=\"$output\" install

            sed '/RTLDLIST=/s@/usr@@g' -i $output/usr/bin/ldd

            popd
            popd

            rm -rf ./glibc
            rm -rf ./glibc-patch

            ## Test glibc

            echo 'Testing glibc'

            echo 'int main(){{}}' | $TARGET-gcc -xc -

            readelf -l a.out | grep ld-linux

            rm -v a.out

            ## Build libstdc++

            pushd ./libstdc++

            mkdir -pv ./build

            pushd ./build

            ../libstdc++-v3/configure \
                --host=\"$TARGET\" \
                --build=\"$(../config.guess)\" \
                --prefix=\"/usr\" \
                --disable-multilib \
                --disable-nls \
                --disable-libstdcxx-pch \
                --with-gxx-include-dir=\"/tools/$TARGET/include/c++/14.2.0\"

            make
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/lib{{stdc++{{,exp,fs}},supc++}}.la

            popd
            popd

            rm -rf ./libstdc++

            ## Build m4

            pushd ./m4

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./m4

            ## Build ncurses

            pushd ./ncurses

            mkdir -pv build

            pushd ./build
              ../configure AWK=gawk
              make -C include
              make -C progs tic
            popd

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(./config.guess)\" \
                --mandir=\"/usr/share/man\" \
                --with-manpage-format=\"normal\" \
                --with-shared \
                --without-normal \
                --with-cxx-shared \
                --without-debug \
                --without-ada \
                --disable-stripping \
                AWK=gawk

            make
            make DESTDIR=\"$output\" TIC_PATH=\"$(pwd)/build/progs/tic\" install

            ln -sv libncursesw.so $output/usr/lib/libncurses.so

            sed -e 's/^#if.*XOPEN.*$/#if 1/' \
                -i $output/usr/include/curses.h

            popd

            rm -rf ./ncurses

            ## Build bash

            pushd ./bash

            ./configure \
                --prefix=\"/usr\" \
                --build=\"$(sh support/config.guess)\" \
                --host=\"$TARGET\" \
                --without-bash-malloc

            make
            make DESTDIR=\"$output\" install

            ln -sv bash $output/bin/sh

            popd

            rm -rf ./bash

            ## Build coreutils

            pushd ./coreutils

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\" \
                --enable-install-program=\"hostname\" \
                --enable-no-install-program=\"kill,uptime\"

            make
            make DESTDIR=\"$output\" install

            mv -v $output/usr/bin/chroot $output/usr/sbin

            mkdir -pv $output/usr/share/man/man8

            mv -v $output/usr/share/man/man1/chroot.1 \
                $output/usr/share/man/man8/chroot.8

            sed -i 's/\"1\"/\"8\"/' \
                $output/usr/share/man/man8/chroot.8

            popd

            rm -rf ./coreutils

            ## Build diffutils

            pushd ./diffutils

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(./build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./diffutils

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
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(./config.guess)\"

            make FILE_COMPILE=\"$(pwd)/build/src/file\"
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/libmagic.la

            popd

            rm -rf ./file

            ## Build findutils

            pushd ./findutils

            ./configure \
                --prefix=\"/usr\" \
                --localstatedir=\"/var/lib/locate\" \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./findutils

            ## Build gawk

            pushd ./gawk

            sed -i 's/extras//' Makefile.in

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./gawk

            ## Build grep

            pushd ./grep

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(./build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./grep

            ## Build gzip

            pushd ./gzip

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./gzip

            ## Build make

            pushd ./make

            ./configure \
                --prefix=\"/usr\" \
                --without-guile \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./make

            ## Build patch

            pushd ./patch

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./patch

            ## Build sed

            pushd ./sed

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(./build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./sed

            ## Build tar

            pushd ./tar

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./tar

            ## Build xz

            pushd ./xz

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\" \
                --build=\"$(build-aux/config.guess)\" \
                --disable-static \
                --docdir=\"/usr/share/doc/xz-5.6.3\"

            make
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/liblzma.la

            popd

            rm -rf ./xz

            ## Build binutils (pass 02)

            pushd ./binutils-pass-02

            sed '6009s/$add_dir//' -i ltmain.sh

            mkdir -pv ./build

            pushd ./build

            ../configure \
                --prefix=\"/usr\" \
                --build=\"$(../config.guess)\" \
                --host=\"$TARGET\" \
                --disable-nls \
                --enable-shared \
                --enable-gprofng=\"no\" \
                --disable-werror \
                --enable-64-bit-bfd \
                --enable-new-dtags \
                --enable-default-hash-style=\"gnu\"

            make
            make DESTDIR=\"$output\" install

            rm -v $output/usr/lib/lib{{bfd,ctf,ctf-nobfd,opcodes,sframe}}.{{a,la}}

            popd
            popd

            rm -rf ./binutils-pass-02

            ## Build gcc (pass 02)

            pushd ./gcc-pass-02

            ./contrib/download_prerequisites

            case $(uname -m) in
              x86_64)
                sed -e '/m64=/s/lib64/lib/' \
                    -i.orig gcc/config/i386/t-linux64
              ;;
            esac

            sed '/thread_header =/s/@.*@/gthr-posix.h/' \
                -i libgcc/Makefile.in libstdc++-v3/include/Makefile.in

            mkdir -pv ./build

            pushd ./build

            ../configure \
                --build=\"$(../config.guess)\" \
                --host=\"$TARGET\" \
                --target=\"$TARGET\" \
                LDFLAGS_FOR_TARGET=\"-L$PWD/$TARGET/libgcc\" \
                --prefix=\"/usr\" \
                --with-build-sysroot=\"$output\" \
                --enable-default-pie \
                --enable-default-ssp \
                --disable-nls \
                --disable-multilib \
                --disable-libatomic \
                --disable-libgomp \
                --disable-libquadmath \
                --disable-libsanitizer \
                --disable-libssp \
                --disable-libvtv \
                --enable-languages=\"c,c++\"

            make
            make DESTDIR=\"$output\" install

            ln -sv gcc $output/usr/bin/cc

            popd
            popd

            rm -rf ./gcc-pass-02
            ",
        },
        source: vec![
            PackageSource {
                excludes: vec![],
                hash: Some(
                    "7dc29154d5344d3d4f943396de2a6c764c36b4729bd76363b9ccf8a5166c07d8".to_string(),
                ),
                includes: vec![],
                name: "bash".to_string(),
                strip_prefix: true,
                uri: "https://ftpmirror.gnu.org/gnu/bash/bash-5.2.37.tar.gz".to_string(),
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
                    "0ad86940ddd48f6e8ebb9605c98e4072a127fabda72dc235ffe94fd984101d00".to_string(),
                ),
                includes: vec![],
                name: "linux-headers".to_string(),
                strip_prefix: true,
                uri: "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.11.6.tar.xz".to_string(),
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
                    "2c7a608231d70ba4d7c81fc70fd1eb81d93c424865eb255a8996f8e9ffcb55ee".to_string(),
                ),
                includes: vec![],
                name: "xz".to_string(),
                strip_prefix: true,
                uri:
                    "https://github.com/tukaani-project/xz/releases/download/v5.6.3/xz-5.6.3.tar.xz"
                        .to_string(),
            },
        ],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    context.add_package(package)
}
