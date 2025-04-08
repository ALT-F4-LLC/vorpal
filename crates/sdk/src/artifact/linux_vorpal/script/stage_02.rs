use indoc::formatdoc;

#[allow(clippy::too_many_arguments)]
pub fn script(
    bash_version: &str,
    binutils_version: &str,
    coreutils_version: &str,
    diffutils_version: &str,
    file_version: &str,
    findutils_version: &str,
    gawk_version: &str,
    gcc_version: &str,
    grep_version: &str,
    gzip_version: &str,
    m4_version: &str,
    make_version: &str,
    ncurses_version: &str,
    patch_version: &str,
    sed_version: &str,
    tar_version: &str,
    xz_version: &str,
) -> String {
    formatdoc! {"
        set +h
        umask 022

        ## Setup environment

        export LC_ALL=\"POSIX\"
        export VORPAL_TARGET=\"$(uname -m)-vorpal-linux-gnu\"
        export PATH=\"$VORPAL_OUTPUT/tools/bin:$PATH\"
        export CONFIG_SITE=\"$VORPAL_OUTPUT/usr/share/config.site\"
        export MAKEFLAGS=\"-j$(nproc)\"
        export VORPAL_SOURCE=\"$(pwd)/source\"

        ## Build m4

        mkdir -pv $VORPAL_SOURCE/m4/m4-{m4_version}/build
        pushd $VORPAL_SOURCE/m4/m4-{m4_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/m4

        ## Build ncurses

        mkdir -pv $VORPAL_SOURCE/ncurses/ncurses-{ncurses_version}
        pushd $VORPAL_SOURCE/ncurses/ncurses-{ncurses_version}

        mkdir -pv build
        pushd build

        ../configure AWK=gawk

        make -C include
        make -C progs tic

        popd

        ./configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
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
        make DESTDIR=\"$VORPAL_OUTPUT\" TIC_PATH=\"$(pwd)/build/progs/tic\" install

        ln -sv libncursesw.so $VORPAL_OUTPUT/usr/lib/libncurses.so

        sed -e 's/^#if.*XOPEN.*$/#if 1/' \
            -i $VORPAL_OUTPUT/usr/include/curses.h

        popd

        rm -rf $VORPAL_SOURCE/ncurses

        ## Build bash

        mkdir -pv $VORPAL_SOURCE/bash/bash-{bash_version}/build
        pushd $VORPAL_SOURCE/bash/bash-{bash_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --build=\"$(sh ../support/config.guess)\" \
            --host=\"$VORPAL_TARGET\" \
            --without-bash-malloc

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        ln -sv bash $VORPAL_OUTPUT/usr/bin/sh

        popd

        rm -rf $VORPAL_SOURCE/bash

        ## Build coreutils

        mkdir -pv $VORPAL_SOURCE/coreutils/coreutils-{coreutils_version}/build
        pushd $VORPAL_SOURCE/coreutils/coreutils-{coreutils_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\" \
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

        rm -rf $VORPAL_SOURCE/coreutils

        ## Build diffutils

        mkdir -pv $VORPAL_SOURCE/diffutils/diffutils-{diffutils_version}/build
        pushd $VORPAL_SOURCE/diffutils/diffutils-{diffutils_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/diffutils

        ## Build file

        mkdir -pv $VORPAL_SOURCE/file/file-{file_version}
        pushd $VORPAL_SOURCE/file/file-{file_version}

        mkdir -pv build
        pushd build

        ../configure \
            --disable-bzlib \
            --disable-libseccomp \
            --disable-xzlib \
            --disable-zlib

        make

        popd

        ./configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(./config.guess)\"

        make FILE_COMPILE=\"$(pwd)/build/src/file\"
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        rm -v $VORPAL_OUTPUT/usr/lib/libmagic.la

        popd

        rm -rf $VORPAL_SOURCE/file

        ## Build findutils

        mkdir -pv $VORPAL_SOURCE/findutils/findutils-{findutils_version}/build
        pushd $VORPAL_SOURCE/findutils/findutils-{findutils_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --localstatedir=\"/var/lib/locate\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/findutils

        ## Build gawk

        mkdir -pv $VORPAL_SOURCE/gawk/gawk-{gawk_version}/build
        pushd $VORPAL_SOURCE/gawk/gawk-{gawk_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/gawk

        ## Build grep

        mkdir -pv $VORPAL_SOURCE/grep/grep-{grep_version}/build
        pushd $VORPAL_SOURCE/grep/grep-{grep_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/grep

        ## Build gzip

        mkdir -pv $VORPAL_SOURCE/gzip/gzip-{gzip_version}/build
        pushd $VORPAL_SOURCE/gzip/gzip-{gzip_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/gzip

        ## Build make

        mkdir -pv $VORPAL_SOURCE/make/make-{make_version}/build
        pushd $VORPAL_SOURCE/make/make-{make_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --without-guile \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/make

        ## Build patch

        mkdir -pv $VORPAL_SOURCE/patch/patch-{patch_version}/build
        pushd $VORPAL_SOURCE/patch/patch-{patch_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/patch

        ## Build sed

        mkdir -pv $VORPAL_SOURCE/sed/sed-{sed_version}/build
        pushd $VORPAL_SOURCE/sed/sed-{sed_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/sed

        ## Build tar

        mkdir -pv $VORPAL_SOURCE/tar/tar-{tar_version}/build
        pushd $VORPAL_SOURCE/tar/tar-{tar_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$($VORPAL_SOURCE/tar/tar-{tar_version}/build-aux/config.guess)\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        popd

        rm -rf $VORPAL_SOURCE/tar

        ## Build xz

        mkdir -pv $VORPAL_SOURCE/xz/xz-{xz_version}/build
        pushd $VORPAL_SOURCE/xz/xz-{xz_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --host=\"$VORPAL_TARGET\" \
            --build=\"$(../build-aux/config.guess)\" \
            --disable-static \
            --docdir=\"/usr/share/doc/xz-5.6.3\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        rm -v $VORPAL_OUTPUT/usr/lib/liblzma.la

        popd

        rm -rf $VORPAL_SOURCE/xz

        ## Build binutils (pass 02)

        mkdir -pv $VORPAL_SOURCE/binutils-pass-02/binutils-{binutils_version}/build
        pushd $VORPAL_SOURCE/binutils-pass-02/binutils-{binutils_version}/build

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

        rm -rf $VORPAL_SOURCE/binutils-pass-02

        ## Build gcc (pass 02)

        mkdir -pv $VORPAL_SOURCE/gcc-pass-02/gcc-{gcc_version}/build
        pushd $VORPAL_SOURCE/gcc-pass-02/gcc-{gcc_version}/build

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

        rm -rf $VORPAL_SOURCE/gcc-pass-02

        ## Setup root symlinks

        ln -svf usr/bin $VORPAL_OUTPUT/bin
        ln -svf usr/lib $VORPAL_OUTPUT/lib
        ln -svf usr/sbin $VORPAL_OUTPUT/sbin",
    }
}
