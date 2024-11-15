use crate::ContextConfig;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

pub fn artifact(
    context: &mut ContextConfig,
    cross_toolchain_rootfs: &ArtifactId,
) -> Result<ArtifactId> {
    let environments = vec![ArtifactEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin".to_string(),
    }];

    let sandbox = Some(cross_toolchain_rootfs.clone());

    let systems = vec![Aarch64Linux.into(), X8664Linux.into()];

    let bash = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-bash".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./bash-{version}.tar.gz https://ftpmirror.gnu.org/gnu/bash/bash-{version}.tar.gz
            tar -xvf ./bash-{version}.tar.gz -C $output --strip-components=1",
            version = "5.2.32",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let binutils = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-binutils".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./binutils-{version}.tar.xz https://ftpmirror.gnu.org/gnu/binutils/binutils-{version}.tar.xz
            tar -xvf ./binutils-{version}.tar.xz -C $output --strip-components=1",
            version = "2.43.1",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let bison = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-bison".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./bison-{version}.tar.xz https://ftpmirror.gnu.org/gnu/bison/bison-{version}.tar.xz
            tar -xvf ./bison-{version}.tar.xz -C $output --strip-components=1",
            version = "3.8.2",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let coreutils = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-coreutils".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./coreutils-{version}.tar.xz https://ftpmirror.gnu.org/gnu/coreutils/coreutils-{version}.tar.xz
            tar -xvf ./coreutils-{version}.tar.xz -C $output --strip-components=1",
            version = "9.5",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let diffutils = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-diffutils".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./diffutils-{version}.tar.xz https://ftpmirror.gnu.org/gnu/diffutils/diffutils-{version}.tar.xz
            tar -xvf ./diffutils-{version}.tar.xz -C $output --strip-components=1",
            version = "3.10",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let file = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-file".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./file-{version}.tar.gz https://astron.com/pub/file/file-{version}.tar.gz
            tar -xvf ./file-{version}.tar.gz -C $output --strip-components=1",
            version = "5.45",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let findutils = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-findutils".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./findutils-{version}.tar.xz https://ftpmirror.gnu.org/gnu/findutils/findutils-{version}.tar.xz
            tar -xvf ./findutils-{version}.tar.xz -C $output --strip-components=1",
            version = "4.10.0",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let gawk = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-gawk".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./gawk-{version}.tar.xz https://ftpmirror.gnu.org/gnu/gawk/gawk-{version}.tar.xz
            tar -xvf ./gawk-{version}.tar.xz -C $output --strip-components=1",
            version = "5.3.0",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let gcc = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-gcc".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./gcc-{version}.tar.xz https://ftpmirror.gnu.org/gnu/gcc/gcc-{version}/gcc-{version}.tar.xz
            tar -xvf ./gcc-{version}.tar.xz -C $output --strip-components=1",
            version = "14.2.0",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let gettext = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-gettext".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./gettext-{version}.tar.xz https://ftpmirror.gnu.org/gnu/gettext/gettext-{version}.tar.xz
            tar -xvf ./gettext-{version}.tar.xz -C $output --strip-components=1",
            version = "0.22.5",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let glibc = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-glibc".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./glibc-{version}.tar.xz https://ftpmirror.gnu.org/gnu/glibc/glibc-{version}.tar.xz
            tar -xvf ./glibc-{version}.tar.xz -C $output --strip-components=1",
            version = "2.40",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let glibc_patch = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-glibc-patch".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./glibc-patch-{version}.patch https://www.linuxfromscratch.org/patches/lfs/12.2/glibc-{version}-fhs-1.patch
            cp -v ./glibc-patch-{version}.patch $output",
            version = "2.40",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let grep = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-grep".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./grep-{version}.tar.xz https://ftpmirror.gnu.org/gnu/grep/grep-{version}.tar.xz
            tar -xvf ./grep-{version}.tar.xz -C $output --strip-components=1",
            version = "3.11",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let gzip = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-gzip".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./gzip-{version}.tar.xz https://ftpmirror.gnu.org/gnu/gzip/gzip-{version}.tar.xz
            tar -xvf ./gzip-{version}.tar.xz -C $output --strip-components=1",
            version = "1.13",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let linux_headers = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-linux-headers".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./linux-headers-{version}.tar.xz https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-{version}.tar.xz
            tar -xvf ./linux-headers-{version}.tar.xz -C $output --strip-components=1",
            version = "6.10.5",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let m4 = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-m4".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./m4-{version}.tar.xz https://ftpmirror.gnu.org/gnu/m4/m4-{version}.tar.xz
            tar -xvf ./m4-{version}.tar.xz -C $output --strip-components=1",
            version = "1.4.19",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let make = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-make".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./make-{version}.tar.gz https://ftpmirror.gnu.org/gnu/make/make-{version}.tar.gz
            tar -xvf ./make-{version}.tar.gz -C $output --strip-components=1",
            version = "4.4.1",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let ncurses = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-ncurses".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./ncurses-{version}.tar.gz https://invisible-mirror.net/archives/ncurses/ncurses-{version}.tar.gz
            tar -xvf ./ncurses-{version}.tar.gz -C $output --strip-components=1",
            version = "6.5",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let patchelf = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-patchelf".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./patchelf-{version}.tar.gz https://github.com/NixOS/patchelf/releases/download/{version}/patchelf-{version}.tar.gz
            tar -xvf ./patchelf-{version}.tar.gz -C $output --strip-components=1",
            version = "0.18.0",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let patch = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-patch".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./patch-{version}.tar.xz https://ftpmirror.gnu.org/gnu/patch/patch-{version}.tar.xz
            tar -xvf ./patch-{version}.tar.xz -C $output --strip-components=1",
            version = "2.7.6",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let perl = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-perl".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./perl-{version}.tar.gz https://www.cpan.org/src/5.0/perl-{version}.tar.xz
            tar -xvf ./perl-{version}.tar.gz -C $output --strip-components=1",
            version = "5.40.0",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let python = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-python".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./python-{version}.tar.xz https://www.python.org/ftp/python/{version}/Python-{version}.tar.xz
            tar -xvf ./python-{version}.tar.xz -C $output --strip-components=1",
            version = "3.12.5",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let sed = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-sed".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./sed-{version}.tar.xz https://ftpmirror.gnu.org/gnu/sed/sed-{version}.tar.xz
            tar -xvf ./sed-{version}.tar.xz -C $output --strip-components=1",
            version = "4.9",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let tar = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-tar".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./tar-{version}.tar.xz https://ftpmirror.gnu.org/gnu/tar/tar-{version}.tar.xz
            tar -xvf ./tar-{version}.tar.xz -C $output --strip-components=1",
            version = "1.35",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let texinfo = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-texinfo".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./texinfo-{version}.tar.xz https://ftpmirror.gnu.org/gnu/texinfo/texinfo-{version}.tar.xz
            tar -xvf ./texinfo-{version}.tar.xz -C $output --strip-components=1",
            version = "7.1.1",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let util_linux = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-util-linux".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./util-linux-{version}.tar.xz https://www.kernel.org/pub/linux/utils/util-linux/v2.40/util-linux-{version}.tar.xz
            tar -xvf ./util-linux-{version}.tar.xz -C $output --strip-components=1",
            version = "2.40.2",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    let xz = context.add_artifact(Artifact {
        artifacts: vec![],
        environments: environments.clone(),
        name: "cross-toolchain-xz".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            curl -L -o ./xz-{version}.tar.xz https://github.com/tukaani-project/xz/releases/download/v{version}/xz-{version}.tar.xz
            tar -xvf ./xz-{version}.tar.xz -C $output --strip-components=1",
            version = "5.6.2",
        },
        sources: vec![],
        systems: systems.clone(),
    })?;

    context.add_artifact(Artifact {
        artifacts: vec![
            bash,
            binutils,
            bison,
            coreutils,
            diffutils,
            file,
            findutils,
            gawk,
            gcc,
            gettext,
            glibc,
            glibc_patch,
            grep,
            gzip,
            linux_headers,
            m4,
            make,
            ncurses,
            patch,
            patchelf,
            perl,
            python,
            sed,
            tar,
            texinfo,
            util_linux,
            xz,
        ],
        environments: environments.clone(),
        name: "cross-toolchain".to_string(),
        sandbox: sandbox.clone(),
        script: formatdoc! {"
            #!/bin/bash
            set -euo +h pipefail
            umask 022

            ### Setup paths

            mkdir -pv $output/{{etc,var}} $output/usr/{{bin,lib,sbin}}

            for i in bin lib sbin; do
              ln -sv usr/$i $output/$i
            done

            case $(uname -m) in
              aarch64) mkdir -pv $output/lib64 ;;
              x86_64) mkdir -pv $output/lib64 ;;
            esac

            mkdir -pv $output/tools

            ### Setup environment

            export LC_ALL=\"POSIX\"
            export TARGET=\"$(uname -m)-vorpal-linux-gnu\"
            export PATH=\"$output/tools/bin:$PATH\"
            export CONFIG_SITE=\"$output/usr/share/config.site\"
            export MAKEFLAGS=\"-j$(nproc)\"

            ### Setup certificates

            mkdir -pv $output/etc/ssl/certs
            mkdir -pv $output/usr/share/ca-certificates/mozilla

            rsync -av /etc/ssl/certs/ca-certificates.crt $output/etc/ssl/certs
            rsync -av /usr/share/ca-certificates/mozilla/* $output/usr/share/ca-certificates/mozilla

            cp -v /etc/ca-certificates.conf $output/etc

            ### Setup resolv.conf

            echo 'nameserver 1.1.1.1' > $output/etc/resolv.conf

            ### Build binutils (pass 01)

            mkdir -pv binutils-pass-01
            rsync -av $cross_toolchain_binutils/ binutils-pass-01/
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

            mkdir -pv gcc-pass-01
            rsync -av $cross_toolchain_gcc/ gcc-pass-01/
            pushd ./gcc-pass-01

            ./contrib/download_prerequisites

            case $(uname -m) in
              aarch64)
                sed -e '/lp64=/s/lib64/lib/' \
                    -i.orig gcc/config/aarch64/t-aarch64-linux
             ;;
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

            mkdir -pv linux-headers
            rsync -av $cross_toolchain_linux_headers/ linux-headers/
            pushd ./linux-headers

            make mrproper
            make headers

            find usr/include -type f ! -name '*.h' -delete

            cp -rv usr/include \"$output/usr\"

            popd

            rm -rf ./linux-headers

            ### Build glibc

            mkdir -pv glibc
            mkdir -pv glibc-patch
            rsync -av $cross_toolchain_glibc/ glibc/
            rsync -av $cross_toolchain_glibc_patch/ glibc-patch/
            pushd ./glibc

            case $(uname -m) in
                aarch64) ln -sfv ../lib/ld-linux-aarch64.so.1 $output/lib64
                ;;
                i?86)   ln -sfv ld-linux.so.2 $output/lib/ld-lsb.so.3
                ;;
                x86_64) ln -sfv ../lib/ld-linux-x86-64.so.2 $output/lib64
                        ln -sfv ../lib/ld-linux-x86-64.so.2 $output/lib64/ld-lsb-x86-64.so.3
                ;;
            esac

            patch -Np1 -i ../glibc-patch/glibc-patch-2.40.patch

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

            mkdir -pv libstdc++
            rsync -av $cross_toolchain_gcc/ libstdc++
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

            mkdir -pv m4
            rsync -av $cross_toolchain_m4/ m4/
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

            mkdir -pv ncurses
            rsync -av $cross_toolchain_ncurses/ ncurses/
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

            mkdir -pv bash
            rsync -av $cross_toolchain_bash/ bash/
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

            mkdir -pv coreutils
            rsync -av $cross_toolchain_coreutils/ coreutils/
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

            mkdir -pv diffutils
            rsync -av $cross_toolchain_diffutils/ diffutils/
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

            mkdir -pv file
            rsync -av $cross_toolchain_file/ file/
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

            mkdir -pv findutils
            rsync -av $cross_toolchain_findutils/ findutils/
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

            mkdir -pv gawk
            rsync -av $cross_toolchain_gawk/ gawk/
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

            mkdir -pv grep
            rsync -av $cross_toolchain_grep/ grep/
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

            mkdir -pv gzip
            rsync -av $cross_toolchain_gzip/ gzip/
            pushd ./gzip

            ./configure \
                --prefix=\"/usr\" \
                --host=\"$TARGET\"

            make
            make DESTDIR=\"$output\" install

            popd

            rm -rf ./gzip

            ## Build make

            mkdir -pv make
            rsync -av $cross_toolchain_make/ make/
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

            mkdir -pv patch
            rsync -av $cross_toolchain_patch/ patch/
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

            mkdir -pv sed
            rsync -av $cross_toolchain_sed/ sed/
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

            mkdir -pv tar
            rsync -av $cross_toolchain_tar/ tar/
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

            mkdir -pv xz
            rsync -av $cross_toolchain_xz/ xz/
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

            mkdir -pv binutils-pass-02
            rsync -av $cross_toolchain_binutils/ binutils-pass-02/
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

            mkdir -pv gcc-pass-02
            rsync -av $cross_toolchain_gcc/ gcc-pass-02/
            pushd ./gcc-pass-02

            ./contrib/download_prerequisites

            case $(uname -m) in
              aarch64)
                sed -e '/lp64=/s/lib64/lib/' \
                    -i.orig gcc/config/aarch64/t-aarch64-linux
              ;;
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

            ### Setup sandbox in sandbox

            mkdir -pv sandbox/home

            mkdir -pv sandbox/source/bison
            mkdir -pv sandbox/source/gettext
            mkdir -pv sandbox/source/patchelf
            mkdir -pv sandbox/source/perl
            mkdir -pv sandbox/source/python
            mkdir -pv sandbox/source/texinfo
            mkdir -pv sandbox/source/util-linux

            rsync -av $cross_toolchain_bison/ sandbox/source/bison/
            rsync -av $cross_toolchain_gettext/ sandbox/source/gettext/
            rsync -av $cross_toolchain_patchelf/ sandbox/source/patchelf/
            rsync -av $cross_toolchain_perl/ sandbox/source/perl/
            rsync -av $cross_toolchain_python/ sandbox/source/python/
            rsync -av $cross_toolchain_texinfo/ sandbox/source/texinfo/
            rsync -av $cross_toolchain_util_linux/ sandbox/source/util-linux/

            cat > $output/etc/hosts << EOF
            127.0.0.1  localhost
            ::1        localhost
            EOF

            cat > $output/etc/passwd << \"EOF\"
            root:x:0:0:root:/root:/bin/bash
            bin:x:1:1:bin:/dev/null:/usr/bin/false
            daemon:x:6:6:Daemon User:/dev/null:/usr/bin/false
            messagebus:x:18:18:D-Bus Message Daemon User:/run/dbus:/usr/bin/false
            uuidd:x:80:80:UUID Generation Daemon User:/dev/null:/usr/bin/false
            nobody:x:65534:65534:Unprivileged User:/dev/null:/usr/bin/false
            EOF

            cat > $output/etc/group << \"EOF\"
            root:x:0:
            bin:x:1:daemon
            sys:x:2:
            kmem:x:3:
            tape:x:4:
            tty:x:5:
            daemon:x:6:
            floppy:x:7:
            disk:x:8:
            lp:x:9:
            dialout:x:10:
            audio:x:11:
            video:x:12:
            utmp:x:13:
            cdrom:x:15:
            adm:x:16:
            messagebus:x:18:
            input:x:24:
            mail:x:34:
            kvm:x:61:
            uuidd:x:80:
            wheel:x:97:
            users:x:999:
            nogroup:x:65534:
            EOF

            cat > sandbox/artifact.sh<< EOF
            #!/bin/bash
            set -euo pipefail

            mkdir -pv /{{boot,home,mnt,opt,srv}}

            mkdir -pv /etc/{{opt,sysconfig}}
            mkdir -pv /lib/firmware
            mkdir -pv /media/{{floppy,cdrom}}
            mkdir -pv /usr/{{,local/}}{{include,src}}
            mkdir -pv /usr/lib/locale
            mkdir -pv /usr/local/{{bin,lib,sbin}}
            mkdir -pv /usr/{{,local/}}share/{{color,dict,doc,info,locale,man}}
            mkdir -pv /usr/{{,local/}}share/{{misc,terminfo,zoneinfo}}
            mkdir -pv /usr/{{,local/}}share/man/man{{1..8}}
            mkdir -pv /var/{{cache,local,log,mail,opt,spool}}
            mkdir -pv /var/lib/{{color,misc,locate}}

            install -dv -m 0750 /root

            localedef -i C -f UTF-8 C.UTF-8

            ## Build gettext

            pushd ./gettext

            ./configure --disable-shared

            make

            cp -v gettext-tools/src/{{msgfmt,msgmerge,xgettext}} /usr/bin

            popd

            rm -rf ./gettext

            ## Build bison

            pushd ./bison

            ./configure \
                --prefix=\"/usr\" \
                --docdir=\"/usr/share/doc/bison-3.8.2\"

            make
            make install

            popd

            rm -rf ./bison

            ## Build perl

            pushd ./perl

            sh Configure \
                -des \
                -D prefix=\"/usr\" \
                -D vendorprefix=\"/usr\" \
                -D useshrplib \
                -D privlib=\"/usr/lib/perl5/5.40/core_perl\" \
                -D archlib=\"/usr/lib/perl5/5.40/core_perl\" \
                -D sitelib=\"/usr/lib/perl5/5.40/site_perl\" \
                -D sitearch=\"/usr/lib/perl5/5.40/site_perl\" \
                -D vendorlib=\"/usr/lib/perl5/5.40/vendor_perl\" \
                -D vendorarch=\"/usr/lib/perl5/5.40/vendor_perl\"

            make
            make install

            popd

            rm -rf ./perl

            ## Build Python

            pushd ./python

            ./configure \
                --prefix=\"/usr\" \
                --enable-shared \
                --without-ensurepip

            make
            make install

            popd

            rm -rf ./python

            ## Build texinfo

            pushd ./texinfo

            ./configure --prefix=\"/usr\"

            make
            make install

            popd

            rm -rf ./texinfo

            ## Build util-linux

            pushd ./util-linux

            mkdir -pv /var/lib/hwclock

            # note: \"--disable-makeinstall-chown\" for bwrap limitations

            ./configure \
                --libdir=\"/usr/lib\" \
                --runstatedir=\"/run\" \
                --disable-chfn-chsh \
                --disable-login \
                --disable-nologin \
                --disable-su \
                --disable-setpriv \
                --disable-runuser \
                --disable-pylibmount \
                --disable-static \
                --disable-liblastlog2 \
                --disable-makeinstall-chown \
                --without-python \
                ADJTIME_PATH=\"/var/lib/hwclock/adjtime\" \
                --docdir=\"/usr/share/doc/util-linux-2.40.2\"

            make
            make install

            popd

            rm -rf ./util-linux

            ## Build patchelf

            pushd ./patchelf

            ./configure --prefix=\"$output\"

            make
            make install

            popd

            rm -rf ./patchelf

            ## Cleanup

            rm -rf /usr/share/{{info,man,doc}}/*

            find /usr/{{lib,libexec}} -name \\*.la -delete

            echo 'Done'
            EOF

            chmod +x sandbox/artifact.sh

            ## Run sandbox

            bwrap \
                --unshare-all \
                --share-net \
                --clearenv \
                --chdir \"$PWD/sandbox/source\" \
                --dev \"/dev\" \
                --proc \"/proc\" \
                --tmpfs \"/tmp\" \
                --gid \"0\" \
                --uid \"0\" \
                --bind \"$PWD\" \"$PWD\" \
                --bind \"$output/bin\" \"/bin\" \
                --bind \"$output/etc\" \"/etc\" \
                --bind \"$output/lib64\" \"/lib64\" \
                --bind \"$output/lib\" \"/lib\" \
                --bind \"$output/sbin\" \"/sbin\" \
                --bind \"$output/usr\" \"/usr\" \
                --bind \"$output\" \"$output\" \
                --setenv \"HOME\" \"$PWD/sandbox/home\" \
                --setenv \"MAKEFLAGS\" \"-j$(nproc)\" \
                --setenv \"PATH\" \"/usr/bin:/usr/sbin\" \
                --setenv \"PS1\" \"(sandbox) \\u:\\w\\$ \" \
                --setenv \"TESTSUITEFLAGS\" \"-j$(nproc)\" \
                --setenv \"output\" \"$output\" \
                $PWD/sandbox/artifact.sh

            rm -rf $output/tools",
        },
        sources: vec![],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    })
}
