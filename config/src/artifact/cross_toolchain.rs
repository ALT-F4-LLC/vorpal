use crate::{
    artifact::{run_bwrap_step, step_env_artifact},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactId,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

pub fn artifact(context: &mut ContextConfig, sandbox: &ArtifactId) -> Result<ArtifactId> {
    let artifacts = vec![sandbox.clone()];

    let sandbox_key = step_env_artifact(sandbox);

    let arguments = vec![
        vec![
            "--ro-bind".to_string(),
            format!("{}/bin", sandbox_key),
            "/bin".to_string(),
        ],
        vec![
            "--ro-bind".to_string(),
            format!("{}/etc", sandbox_key),
            "/etc".to_string(),
        ],
        vec![
            "--ro-bind".to_string(),
            format!("{}/lib", sandbox_key),
            "/lib".to_string(),
        ],
        vec![
            "--ro-bind-try".to_string(),
            format!("{}/lib64", sandbox_key),
            "/lib64".to_string(),
        ],
        vec![
            "--ro-bind".to_string(),
            format!("{}/sbin", sandbox_key),
            "/sbin".to_string(),
        ],
        vec![
            "--ro-bind".to_string(),
            format!("{}/usr", sandbox_key),
            "/usr".to_string(),
        ],
        vec![
            "--setenv".to_string(),
            "PATH".to_string(),
            "/usr/bin:/usr/sbin".to_string(),
        ],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<String>>();

    let systems = vec![Aarch64Linux.into(), X8664Linux.into()];

    let bash = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "bash-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./bash-{version}.tar.gz \
                    https://ftpmirror.gnu.org/gnu/bash/bash-{version}.tar.gz
                tar -xvf ./bash-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
                version = "5.2.32",
            },
        )],
        systems: systems.clone(),
    })?;

    let binutils = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "binutils-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./binutils-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/binutils/binutils-{version}.tar.xz
                tar -xvf ./binutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "2.43.1",
            },
        )],
        systems: systems.clone(),
    })?;

    let coreutils = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "coreutils-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./coreutils-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/coreutils/coreutils-{version}.tar.xz
                tar -xvf ./coreutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "9.5",
            },
        )],
        systems: systems.clone(),
    })?;

    let curl = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "curl-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./curl-{version}.tar.xz \
                    https://curl.se/download/curl-{version}.tar.xz
                tar -xvf ./curl-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "8.11.0",
            },
        )],
        systems: systems.clone(),
    })?;

    let diffutils = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "diffutils-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./diffutils-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/diffutils/diffutils-{version}.tar.xz
                tar -xvf ./diffutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "3.10",
            },
        )],
        systems: systems.clone(),
    })?;

    let file = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "file-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./file-{version}.tar.gz \
                    https://astron.com/pub/file/file-{version}.tar.gz
                tar -xvf ./file-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
                version = "5.45",
            },
        )],
        systems: systems.clone(),
    })?;

    let findutils = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "findutils-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./findutils-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/findutils/findutils-{version}.tar.xz
                tar -xvf ./findutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "4.10.0",
            },
        )],
        systems: systems.clone(),
    })?;

    let gawk = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "gawk-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./gawk-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/gawk/gawk-{version}.tar.xz
                tar -xvf ./gawk-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1
                sed -i 's/extras//' $VORPAL_OUTPUT/Makefile.in",
                version = "5.3.0",
            },
        )],
        systems: systems.clone(),
    })?;

    let gcc = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "gcc-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./gcc-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/gcc/gcc-{version}/gcc-{version}.tar.xz
                tar -xvf ./gcc-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1
                pushd $VORPAL_OUTPUT
                ./contrib/download_prerequisites
                sed -e '/lp64=/s/lib64/lib/' \
                    -i.orig $VORPAL_OUTPUT/gcc/config/aarch64/t-aarch64-linux
                sed -e '/m64=/s/lib64/lib/' \
                    -i.orig $VORPAL_OUTPUT/gcc/config/i386/t-linux64",
                version = "14.2.0",
            },
        )],
        systems: systems.clone(),
    })?;

    let glibc_patch = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "glibc-patch-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./glibc-patch-{version}.patch \
                    https://www.linuxfromscratch.org/patches/lfs/12.2/glibc-{version}-fhs-1.patch
                cp -v ./glibc-patch-{version}.patch $VORPAL_OUTPUT",
                version = "2.40",
            },
        )],
        systems: systems.clone(),
    })?;

    let glibc_artifacts = vec![artifacts.clone(), vec![glibc_patch.clone()]].concat();

    let glibc = context.add_artifact(Artifact {
        artifacts: glibc_artifacts.clone(),
        name: "glibc-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            glibc_artifacts,
            vec![],
            formatdoc! {"
                curl -L -o ./glibc-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/glibc/glibc-{version}.tar.xz
                tar -xvf ./glibc-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1
                pushd $VORPAL_OUTPUT
                patch -Np1 -i {glibc_patch}/glibc-patch-2.40.patch",
                glibc_patch = step_env_artifact(&glibc_patch),
                version = "2.40",
            },
        )],
        systems: systems.clone(),
    })?;

    let grep = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "grep-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./grep-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/grep/grep-{version}.tar.xz
                tar -xvf ./grep-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "3.11",
            },
        )],
        systems: systems.clone(),
    })?;

    let gzip = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "gzip-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./gzip-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/gzip/gzip-{version}.tar.xz
                tar -xvf ./gzip-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "1.13",
            },
        )],
        systems: systems.clone(),
    })?;

    let linux_headers = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "linux-headers-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./linux-headers-{version}.tar.xz \
                    https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-{version}.tar.xz
                tar -xvf ./linux-headers-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "6.10.5",
            },
        )],
        systems: systems.clone(),
    })?;

    let m4 = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "m4-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./m4-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/m4/m4-{version}.tar.xz
                tar -xvf ./m4-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "1.4.19",
            },
        )],
        systems: systems.clone(),
    })?;

    let make = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "make-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./make-{version}.tar.gz \
                    https://ftpmirror.gnu.org/gnu/make/make-{version}.tar.gz
                tar -xvf ./make-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
                version = "4.4.1",
            },
        )],
        systems: systems.clone(),
    })?;

    let ncurses = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "ncurses-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./ncurses-{version}.tar.gz \
                    https://invisible-mirror.net/archives/ncurses/ncurses-{version}.tar.gz
                tar -xvf ./ncurses-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
                version = "6.5",
            },
        )],
        systems: systems.clone(),
    })?;

    let patch = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "patch-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./patch-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/patch/patch-{version}.tar.xz
                tar -xvf ./patch-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "2.7.6",
            },
        )],
        systems: systems.clone(),
    })?;

    let sed = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "sed-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./sed-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/sed/sed-{version}.tar.xz
                tar -xvf ./sed-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "4.9",
            },
        )],
        systems: systems.clone(),
    })?;

    let tar = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "tar-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./tar-{version}.tar.xz \
                    https://ftpmirror.gnu.org/gnu/tar/tar-{version}.tar.xz
                tar -xvf ./tar-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "1.35",
            },
        )],
        systems: systems.clone(),
    })?;

    let xz = context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "xz-source".to_string(),
        sources: vec![],
        steps: vec![run_bwrap_step(
            arguments.clone(),
            artifacts.clone(),
            vec![],
            formatdoc! {"
                curl -L -o ./xz-{version}.tar.xz \
                    https://github.com/tukaani-project/xz/releases/download/v{version}/xz-{version}.tar.xz
                tar -xvf ./xz-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
                version = "5.6.2",
            },
        )],
        systems: systems.clone(),
    })?;

    let artifacts = vec![
        bash.clone(),
        binutils.clone(),
        coreutils.clone(),
        curl.clone(),
        diffutils.clone(),
        file.clone(),
        findutils.clone(),
        gawk.clone(),
        gcc.clone(),
        glibc.clone(),
        grep.clone(),
        gzip.clone(),
        linux_headers.clone(),
        m4.clone(),
        make.clone(),
        ncurses.clone(),
        patch.clone(),
        sandbox.clone(),
        sed.clone(),
        tar.clone(),
        xz.clone(),
    ];

    context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "cross-toolchain".to_string(),
        sources: vec![],
        steps: vec![
            run_bwrap_step(
                arguments.clone(),
                artifacts,
                vec![],
                formatdoc! {"
                    set +h
                    umask 022

                    ### Setup paths

                    mkdir -pv $VORPAL_OUTPUT/{{etc,var}} $VORPAL_OUTPUT/usr/{{bin,lib,sbin}}

                    for $i in bin lib sbin; do
                      ln -sv usr/$i $VORPAL_OUTPUT/$i
                    done

                    case $(uname -m) in
                      aarch64) mkdir -pv $VORPAL_OUTPUT/lib64 ;;
                      x86_64) mkdir -pv $VORPAL_OUTPUT/lib64 ;;
                    esac

                    mkdir -pv $VORPAL_OUTPUT/tools

                    ### Setup environment

                    export LC_ALL=\"POSIX\"
                    export VORPAL_TARGET=\"$(uname -m)-vorpal-linux-gnu\"
                    export PATH=\"$VORPAL_OUTPUT/tools/bin:$PATH\"
                    export CONFIG_SITE=\"$VORPAL_OUTPUT/usr/share/config.site\"
                    export MAKEFLAGS=\"-j$(nproc)\"

                    ### Build binutils (pass 01)

                    mkdir -pv ./binutils-pass-01
                    pushd ./binutils-pass-01

                    {binutils}/configure \
                        --prefix=\"$VORPAL_OUTPUT/tools\" \
                        --with-sysroot=\"$VORPAL_OUTPUT\" \
                        --target=\"$VORPAL_TARGET\" \
                        --disable-nls \
                        --enable-gprofng=\"no\" \
                        --disable-werror \
                        --enable-new-dtags \
                        --enable-default-hash-style=\"gnu\"

                    make
                    make install

                    popd
                    rm -rf ./binutils-pass-01

                    ### Build gcc (pass 01)

                    mkdir -pv ./gcc-pass-01
                    pushd ./gcc-pass-01

                    {gcc}/configure \
                        --target=\"$VORPAL_TARGET\" \
                        --prefix=\"$VORPAL_OUTPUT/tools\" \
                        --with-glibc-version=\"2.40\" \
                        --with-sysroot=\"$VORPAL_OUTPUT\" \
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

                    OUTPUT_LIBGCC=$($VORPAL_TARGET-gcc -print-libgcc-file-name)
                    OUTPUT_LIBGCC_DIR=$(dirname \"${{OUTPUT_LIBGCC}}\")
                    OUTPUT_LIMITS_PATH=${{OUTPUT_LIBGCC_DIR}}/include/limits.h

                    echo \"OUTPUT_LIBGCC: ${{OUTPUT_LIBGCC}}\"
                    echo \"OUTPUT_LIBGCC_DIR: ${{OUTPUT_LIBGCC_DIR}}\"
                    echo \"OUTPUT_LIMITS_PATH: ${{OUTPUT_LIMITS_PATH}}\"

                    cat \
                        {gcc}/gcc/limitx.h \
                        {gcc}/gcc/glimits.h \
                        {gcc}/gcc/limity.h \
                        > $OUTPUT_LIMITS_PATH

                    popd
                    rm -rf ./gcc-pass-01

                    ### Build linux headers

                    mkdir -pv ./linux-headers
                    rsync -av {linux_headers}/ linux-headers/
                    pushd ./linux-headers

                    make mrproper
                    make headers

                    find usr/include -type f ! -name '*.h' -delete
                    cp -rv usr/include \"$VORPAL_OUTPUT/usr\"

                    popd
                    rm -rf ./linux-headers

                    ### Build glibc

                    case $(uname -m) in
                        aarch64) ln -sfv ../lib/ld-linux-aarch64.so.1 $VORPAL_OUTPUT/lib64
                        ;;
                        i?86)   ln -sfv ld-linux.so.2 $VORPAL_OUTPUT/lib/ld-lsb.so.3
                        ;;
                        x86_64) ln -sfv ../lib/ld-linux-x86-64.so.2 $VORPAL_OUTPUT/lib64
                                ln -sfv ../lib/ld-linux-x86-64.so.2 $VORPAL_OUTPUT/lib64/ld-lsb-x86-64.so.3
                        ;;
                    esac

                    mkdir -pv glibc
                    pushd ./glibc

                    echo \"rootsbindir=/usr/sbin\" > configparms

                    {glibc}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({glibc}/scripts/config.guess)\" \
                        --enable-kernel=\"4.19\" \
                        --with-headers=\"$VORPAL_OUTPUT/usr/include\" \
                        --disable-nscd \
                        libc_cv_slibdir=\"/usr/lib\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    sed '/RTLDLIST=/s@/usr@@g' -i $VORPAL_OUTPUT/usr/bin/ldd

                    popd
                    rm -rf ./glibc

                    ## Test glibc

                    echo 'Testing glibc'
                    echo 'int main(){{}}' | $VORPAL_TARGET-gcc -xc -

                    readelf -l a.out | grep ld-linux

                    rm -v a.out

                    ## Build libstdc++

                    mkdir -pv libstdc++
                    pushd ./libstdc++

                    {gcc}/libstdc++-v3/configure \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({gcc}/config.guess)\" \
                        --prefix=\"/usr\" \
                        --disable-multilib \
                        --disable-nls \
                        --disable-libstdcxx-pch \
                        --with-gxx-include-dir=\"/tools/$VORPAL_TARGET/include/c++/14.2.0\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    rm -v $VORPAL_OUTPUT/usr/lib/lib{{stdc++{{,exp,fs}},supc++}}.la

                    popd
                    rm -rf ./libstdc++

                    ## Build m4

                    mkdir -pv m4
                    pushd ./m4

                    {m4}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({m4}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./m4

                    ## Build ncurses

                    mkdir -pv ./ncurses
                    pushd ./ncurses

                    mkdir -pv ./build
                    pushd ./build

                    {ncurses}/configure AWK=gawk

                    make -C include
                    make -C progs tic

                    popd

                    {ncurses}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({ncurses}/config.guess)\" \
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
                    make DESTDIR=\"$VORPAL_OUTPUT\" TIC_PATH=\"$(pwd)/build/progs/tic\" install

                    ln -sv libncursesw.so $VORPAL_OUTPUT/usr/lib/libncurses.so

                    sed -e 's/^#if.*XOPEN.*$/#if 1/' \
                        -i $VORPAL_OUTPUT/usr/include/curses.h

                    popd
                    rm -rf ./ncurses

                    ## Build bash

                    mkdir -pv ./bash
                    pushd ./bash

                    {bash}/configure \
                        --prefix=\"/usr\" \
                        --build=\"$(sh {bash}/support/config.guess)\" \
                        --host=\"$VORPAL_TARGET\" \
                        --without-bash-malloc

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    ln -sv bash $VORPAL_OUTPUT/usr/bin/sh

                    popd
                    rm -rf ./bash

                    ## Build coreutils

                    mkdir -pv ./coreutils
                    pushd ./coreutils

                    {coreutils}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({coreutils}/build-aux/config.guess)\" \
                        --enable-install-program=\"hostname\" \
                        --enable-no-install-program=\"kill,uptime\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    mv -v $VORPAL_OUTPUT/usr/bin/chroot $VORPAL_OUTPUT/usr/sbin

                    mkdir -pv $VORPAL_OUTPUT/usr/share/man/man8

                    mv -v $VORPAL_OUTPUT/usr/share/man/man1/chroot.1 \
                        $VORPAL_OUTPUT/usr/share/man/man8/chroot.8

                    sed -i 's/\"1\"/\"8\"/' \
                        $VORPAL_OUTPUT/usr/share/man/man8/chroot.8

                    popd
                    rm -rf ./coreutils

                    ## Build diffutils

                    mkdir -pv ./diffutils
                    pushd ./diffutils

                    {diffutils}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({diffutils}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./diffutils

                    ## Build file

                    mkdir -pv ./file
                    pushd ./file

                    mkdir -pv ./build
                    pushd ./build

                    {file}/configure \
                        --disable-bzlib \
                        --disable-libseccomp \
                        --disable-xzlib \
                        --disable-zlib

                    make

                    popd

                    {file}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({file}/config.guess)\"

                    make FILE_COMPILE=\"$(pwd)/build/src/file\"
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    rm -v $VORPAL_OUTPUT/usr/lib/libmagic.la

                    popd
                    rm -rf ./file

                    ## Build findutils

                    mkdir -pv ./findutils
                    pushd ./findutils

                    {findutils}/configure \
                        --prefix=\"/usr\" \
                        --localstatedir=\"/var/lib/locate\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({findutils}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./findutils

                    ## Build gawk

                    mkdir -pv ./gawk
                    pushd ./gawk

                    {gawk}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({gawk}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./gawk

                    ## Build grep

                    mkdir -pv grep
                    pushd ./grep

                    {grep}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({grep}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./grep

                    ## Build gzip

                    mkdir -pv ./gzip
                    pushd ./gzip

                    {gzip}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./gzip

                    ## Build make

                    mkdir -pv ./make
                    pushd ./make

                    {make}/configure \
                        --prefix=\"/usr\" \
                        --without-guile \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({make}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./make

                    ## Build patch

                    mkdir -pv ./patch
                    pushd ./patch

                    {patch}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({patch}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./patch

                    ## Build sed

                    mkdir -pv ./sed
                    pushd ./sed

                    {sed}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({sed}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./sed

                    ## Build tar

                    mkdir -pv ./tar
                    pushd ./tar

                    {tar}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({tar}/build-aux/config.guess)\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    popd
                    rm -rf ./tar

                    ## Build xz

                    mkdir -pv ./xz
                    pushd ./xz

                    {xz}/configure \
                        --prefix=\"/usr\" \
                        --host=\"$VORPAL_TARGET\" \
                        --build=\"$({xz}/build-aux/config.guess)\" \
                        --disable-static \
                        --docdir=\"/usr/share/doc/xz-5.6.3\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    rm -v $VORPAL_OUTPUT/usr/lib/liblzma.la

                    popd
                    rm -rf ./xz

                    ## Build binutils (pass 02)

                    mkdir -pv binutils-pass-02
                    rsync -av {binutils}/ binutils-pass-02/
                    pushd ./binutils-pass-02

                    sed '6009s/$add_dir//' -i ltmain.sh

                    mkdir -pv ./build
                    pushd ./build

                    ../configure \
                        --prefix=\"/usr\" \
                        --build=\"$(../config.guess)\" \
                        --host=\"$VORPAL_TARGET\" \
                        --disable-nls \
                        --enable-shared \
                        --enable-gprofng=\"no\" \
                        --disable-werror \
                        --enable-64-bit-bfd \
                        --enable-new-dtags \
                        --enable-default-hash-style=\"gnu\"

                    make
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    rm -v $VORPAL_OUTPUT/usr/lib/lib{{bfd,ctf,ctf-nobfd,opcodes,sframe}}.{{a,la}}

                    popd
                    popd
                    rm -rf ./binutils-pass-02

                    ## Build gcc (pass 02)

                    mkdir -pv ./gcc-pass-02
                    rsync -av {gcc}/ gcc-pass-02/
                    pushd ./gcc-pass-02

                    sed '/thread_header =/s/@.*@/gthr-posix.h/' \
                        -i libgcc/Makefile.in libstdc++-v3/include/Makefile.in

                    mkdir -pv ./build
                    pushd ./build

                    ../configure \
                        --build=\"$(../config.guess)\" \
                        --host=\"$VORPAL_TARGET\" \
                        --target=\"$VORPAL_TARGET\" \
                        LDFLAGS_FOR_TARGET=\"-L$PWD/$VORPAL_TARGET/libgcc\" \
                        --prefix=\"/usr\" \
                        --with-build-sysroot=\"$VORPAL_OUTPUT\" \
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
                    make DESTDIR=\"$VORPAL_OUTPUT\" install

                    ln -sv gcc $VORPAL_OUTPUT/usr/bin/cc

                    popd
                    popd
                    rm -rf ./gcc-pass-02

                    ## Setup root symlinks

                    ln -svf usr/bin $VORPAL_OUTPUT/bin
                    ln -svf usr/lib $VORPAL_OUTPUT/lib
                    ln -svf usr/sbin $VORPAL_OUTPUT/sbin

                    ## Cleanup root directories

                    rm -rfv $VORPAL_OUTPUT/tools
                    rm -rfv $VORPAL_OUTPUT/var",
                    bash = step_env_artifact(&bash),
                    binutils = step_env_artifact(&binutils),
                    coreutils = step_env_artifact(&coreutils),
                    diffutils = step_env_artifact(&diffutils),
                    file = step_env_artifact(&file),
                    findutils = step_env_artifact(&findutils),
                    gawk = step_env_artifact(&gawk),
                    gcc = step_env_artifact(&gcc),
                    glibc = step_env_artifact(&glibc),
                    grep = step_env_artifact(&grep),
                    gzip = step_env_artifact(&gzip),
                    linux_headers = step_env_artifact(&linux_headers),
                    m4 = step_env_artifact(&m4),
                    make = step_env_artifact(&make),
                    ncurses = step_env_artifact(&ncurses),
                    patch = step_env_artifact(&patch),
                    sed = step_env_artifact(&sed),
                    tar = step_env_artifact(&tar),
                    xz = step_env_artifact(&xz),
                }
            ),
            run_bwrap_step(
                vec![
                    vec![
                        "--bind".to_string(),
                        "$VORPAL_OUTPUT/bin".to_string(),
                        "/bin".to_string(),
                    ],
                    vec![
                        "--bind".to_string(),
                        "$VORPAL_OUTPUT/etc".to_string(),
                        "/etc".to_string(),
                    ],
                    vec![
                        "--bind-try".to_string(),
                        "$VORPAL_OUTPUT/lib64".to_string(),
                        "/lib64".to_string(),
                    ],
                    vec![
                        "--bind".to_string(),
                        "$VORPAL_OUTPUT/lib".to_string(),
                        "/lib".to_string(),
                    ],
                    vec![
                        "--bind".to_string(),
                        "$VORPAL_OUTPUT/sbin".to_string(),
                        "/sbin".to_string(),
                    ],
                    vec![
                        "--bind".to_string(),
                        "$VORPAL_OUTPUT/usr".to_string(),
                        "/usr".to_string(),
                    ],
                    vec![
                        "--setenv".to_string(),
                        "PATH".to_string(),
                        "/usr/bin:/usr/sbin".to_string(),
                    ],
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<String>>(),
                vec![],
                vec![],
                formatdoc! {"
                    ## Setup system directories

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

                    ## Setup root

                    install -dv -m 0750 /root

                    ## Setup system files

                    cat > /etc/hosts << \"EOF\"
                    127.0.0.1  localhost
                    ::1        localhost
                    EOF

                    cat > /etc/passwd << \"EOF\"
                    root:x:0:0:root:/root:/bin/bash
                    bin:x:1:1:bin:/dev/null:/usr/bin/false
                    daemon:x:6:6:Daemon User:/dev/null:/usr/bin/false
                    messagebus:x:18:18:D-Bus Message Daemon User:/run/dbus:/usr/bin/false
                    uuidd:x:80:80:UUID Generation Daemon User:/dev/null:/usr/bin/false
                    nobody:x:65534:65534:Unprivileged User:/dev/null:/usr/bin/false
                    EOF

                    cat > /etc/group << \"EOF\"
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

                    ## Setup locale

                    localedef -i C -f UTF-8 C.UTF-8

                    ## Setup logs

                    touch /var/log/{{btmp,lastlog,faillog,wtmp}}

                    ## Setup resolv.conf

                    echo 'nameserver 1.1.1.1' > /etc/resolv.conf

                    ### TODO: build gettext

                    ## Cleanup

                    rm -rfv /usr/share/{{info,man,doc}}/*

                    find /usr/{{lib,libexec}} -name \\*.la -delete"
                }
            ),
        ],
        systems: systems.clone(),
    })
}
