use crate::config::artifact::get_artifact_envkey;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

#[allow(clippy::too_many_arguments)]
pub fn generate(
    bash: &ArtifactId,
    binutils: &ArtifactId,
    coreutils: &ArtifactId,
    diffutils: &ArtifactId,
    file: &ArtifactId,
    findutils: &ArtifactId,
    gawk: &ArtifactId,
    gcc: &ArtifactId,
    glibc: &ArtifactId,
    grep: &ArtifactId,
    gzip: &ArtifactId,
    linux_headers: &ArtifactId,
    m4: &ArtifactId,
    make: &ArtifactId,
    ncurses: &ArtifactId,
    patch: &ArtifactId,
    sed: &ArtifactId,
    tar: &ArtifactId,
    xz: &ArtifactId,
) -> String {
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
        cp -prv {linux_headers}/. linux-headers/
        pushd ./linux-headers

        make mrproper
        make headers

        find usr/include -type f ! -name '*.h' -delete
        cp -prv usr/include \"$VORPAL_OUTPUT/usr\"

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
        cp -prv {binutils}/. binutils-pass-02/
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
        cp -prv {gcc}/. gcc-pass-02/
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
        bash = get_artifact_envkey(bash),
        binutils = get_artifact_envkey(binutils),
        coreutils = get_artifact_envkey(coreutils),
        diffutils = get_artifact_envkey(diffutils),
        file = get_artifact_envkey(file),
        findutils = get_artifact_envkey(findutils),
        gawk = get_artifact_envkey(gawk),
        gcc = get_artifact_envkey(gcc),
        glibc = get_artifact_envkey(glibc),
        grep = get_artifact_envkey(grep),
        gzip = get_artifact_envkey(gzip),
        linux_headers = get_artifact_envkey(linux_headers),
        m4 = get_artifact_envkey(m4),
        make = get_artifact_envkey(make),
        ncurses = get_artifact_envkey(ncurses),
        patch = get_artifact_envkey(patch),
        sed = get_artifact_envkey(sed),
        tar = get_artifact_envkey(tar),
        xz = get_artifact_envkey(xz),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn generate_post(
    bison: &ArtifactId,
    curl: &ArtifactId,
    curl_cacert: &ArtifactId,
    gettext: &ArtifactId,
    libidn2: &ArtifactId,
    libpsl: &ArtifactId,
    libunistring: &ArtifactId,
    openssl: &ArtifactId,
    perl: &ArtifactId,
    python: &ArtifactId,
    texinfo: &ArtifactId,
    unzip: &ArtifactId,
    util_linux: &ArtifactId,
    zlib: &ArtifactId,
) -> String {
    formatdoc! {"
        ## Setup environment

        export MAKEFLAGS=\"-j$(nproc)\"

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

        ## Build gettext

        mkdir -pv ./gettext
        pushd ./gettext

        {gettext}/configure --disable-shared

        make

        cp -pv gettext-tools/src/{{msgfmt,msgmerge,xgettext}} /usr/bin

        popd
        rm -rf ./gettext

        ## Build bison

        mkdir -pv ./bison
        pushd ./bison

        {bison}/configure \
            --prefix=\"/usr\" \
            --docdir=\"/usr/share/doc/bison-3.8.2\"

        make
        make install

        popd
        rm -rf ./bison

        ## Build perl

        mkdir -pv ./perl
        cp -prv {perl}/. perl/
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

        mkdir -pv ./python
        pushd ./python

        {python}/configure \
            --prefix=\"/usr\" \
            --enable-shared \
            --without-ensurepip

        make
        make install

        popd
        rm -rf ./python

        ## Build texinfo

        mkdir -pv ./texinfo
        pushd ./texinfo

        {texinfo}/configure --prefix=\"/usr\"

        make
        make install

        popd
        rm -rf ./texinfo

        ## Build util-linux

        mkdir -pv ./util-linux
        pushd ./util-linux

        mkdir -pv /var/lib/hwclock

        # note: \"--disable-makeinstall-chown\" for sandbox limitations

        {util_linux}/configure \
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

        ## Build zlib

        mkdir -pv ./zlib
        pushd ./zlib

        {zlib}/configure \
            --prefix=\"/usr\"

        make
        # make check
        make install

        rm -fv /usr/lib/libz.a

        popd
        rm -rf ./zlib

        ## Build openssl

        mkdir -pv ./openssl
        pushd ./openssl

        {openssl}/config \
            --prefix=\"/usr\" \
            --openssldir=\"/etc/ssl\" \
            --libdir=\"lib\" \
            shared \
            zlib-dynamic

        make

        # HARNESS_JOBS=$(nproc) make test

        sed -i '/INSTALL_LIBS/s/libcrypto.a libssl.a//' Makefile

        make MANSUFFIX=ssl install

        mv -v /usr/share/doc/openssl /usr/share/doc/openssl-3.3.1
        cp -pfrv doc/* /usr/share/doc/openssl-3.3.1

        popd
        rm -rf ./openssl

        ## END OF STANDARD
        ## START OF EXTRAS

        ## Build libunistring

        mkdir -pv ./libunistring
        pushd ./libunistring

        {libunistring}/configure \
            --prefix=\"/usr\" \
            --disable-static \
            --docdir=\"/usr/share/doc/libunistring-1.2\"

        make
        make install

        popd
        rm -rf ./libunistring

        ## Build libidn2

        mkdir -pv ./libidn2
        pushd ./libidn2

        {libidn2}/configure \
            --prefix=\"/usr\" \
            --disable-static

        make
        make install

        popd
        rm -rf ./libidn2

        ## Build libpsl

        mkdir -pv ./libpsl
        pushd ./libpsl

        {libpsl}/configure --prefix=\"/usr\"

        make
        make install

        popd
        rm -rf ./libpsl

        ## Build CA certificates

        cp -pv {curl_cacert}/etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

        ## Build curl

        mkdir -pv ./curl
        pushd ./curl

        {curl}/configure \
            --prefix=\"/usr\" \
            --disable-static \
            --with-openssl \
            --enable-threaded-resolver \
            --with-ca-path=\"/etc/ssl/certs\"

        make
        make install

        popd
        rm -rf ./curl

        ## Build unzip

        mkdir -pv ./unzip
        cp -prv {unzip}/. unzip/
        pushd ./unzip

        make -f unix/Makefile generic

        make prefix=/usr MANDIR=/usr/share/man/man1 \
            -f unix/Makefile install

        popd
        rm -rf ./unzip

        ## Cleanup

        rm -rfv /usr/share/{{info,man,doc}}/*

        find /usr/{{lib,libexec}} -name \\*.la -delete",
        bison = get_artifact_envkey(bison),
        curl = get_artifact_envkey(curl),
        curl_cacert = get_artifact_envkey(curl_cacert),
        gettext = get_artifact_envkey(gettext),
        libidn2 = get_artifact_envkey(libidn2),
        libpsl = get_artifact_envkey(libpsl),
        libunistring = get_artifact_envkey(libunistring),
        openssl = get_artifact_envkey(openssl),
        perl = get_artifact_envkey(perl),
        python = get_artifact_envkey(python),
        texinfo = get_artifact_envkey(texinfo),
        unzip = get_artifact_envkey(unzip),
        util_linux = get_artifact_envkey(util_linux),
        zlib = get_artifact_envkey(zlib),
    }
}
