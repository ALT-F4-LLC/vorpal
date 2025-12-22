use indoc::formatdoc;

pub fn script(
    binutils_version: &str,
    gcc_version: &str,
    glibc_version: &str,
    openssl_version: &str,
    zlib_version: &str,
) -> String {
    formatdoc! {"
        ## Setup paths

        export VORPAL_SOURCE=\"$(pwd)/source\"

        ## Setup environment

        export MAKEFLAGS=\"-j$(nproc)\"

        ## Build glibc-pass-02

        mkdir -pv $VORPAL_SOURCE/glibc-pass-02/glibc-{glibc_version}/build
        pushd $VORPAL_SOURCE/glibc-pass-02/glibc-{glibc_version}/build

        echo 'rootsbindir=/usr/sbin' > configparms

        ../configure \
            --prefix=/usr \
            --disable-werror \
            --enable-kernel=5.4 \
            --enable-stack-protector=strong \
            --disable-nscd \
            libc_cv_slibdir=/usr/lib

        make

        touch /etc/ld.so.conf

        sed '/test-installation/s@$(PERL)@echo not running@' -i ../Makefile

        make install

        sed '/RTLDLIST=/s@/usr@@g' -i /usr/bin/ldd

        make localedata/install-locales

        cat > /etc/nsswitch.conf << \"EOF\"
        # Begin /etc/nsswitch.conf

        passwd: files
        group: files
        shadow: files

        hosts: files dns
        networks: files

        protocols: files
        services: files
        ethers: files
        rpc: files

        # End /etc/nsswitch.conf
        EOF

        cat > /etc/ld.so.conf << \"EOF\"
        # Begin /etc/ld.so.conf
        /usr/local/lib
        /opt/lib

        EOF

        cat >> /etc/ld.so.conf << \"EOF\"
        # Add an include directory
        include /etc/ld.so.conf.d/*.conf

        EOF

        mkdir -pv /etc/ld.so.conf.d

        popd

        rm -rf $VORPAL_SOURCE/glibc-pass-02

        ## Build zlib

        mkdir -pv $VORPAL_SOURCE/zlib/zlib-{zlib_version}/build
        pushd $VORPAL_SOURCE/zlib/zlib-{zlib_version}/build

        ../configure --prefix=\"/usr\"

        make
        make install

        rm -rf /usr/lib/libz.a

        popd

        rm -rf $VORPAL_SOURCE/zlib

        ## Build binutils-pass-03

        mkdir -pv $VORPAL_SOURCE/binutils-pass-03/binutils-{binutils_version}/build
        pushd $VORPAL_SOURCE/binutils-pass-03/binutils-{binutils_version}/build

        ../configure \
            --prefix=/usr \
            --sysconfdir=/etc \
            --enable-ld=default \
            --enable-plugins \
            --enable-shared \
            --disable-werror \
            --enable-64-bit-bfd \
            --enable-new-dtags \
            --with-system-zlib \
            --enable-default-hash-style=gnu

        make tooldir=/usr
        make tooldir=/usr install

        rm -rf /usr/lib/lib{{bfd,ctf,ctf-nobfd,gprofng,opcodes,sframe}}.a \
            /usr/share/doc/gprofng/

        popd

        rm -rf $VORPAL_SOURCE/binutils-pass-03

        ## Build gcc-pass-03

        mkdir -pv $VORPAL_SOURCE/gcc-pass-03/gcc-{gcc_version}/build
        pushd $VORPAL_SOURCE/gcc-pass-03/gcc-{gcc_version}/build

        ../configure \
            --prefix=/usr \
            LD=ld \
            --enable-languages=c,c++ \
            --enable-default-pie \
            --enable-default-ssp \
            --enable-host-pie \
            --disable-multilib \
            --disable-bootstrap \
            --disable-fixincludes \
            --with-system-zlib

        make

        ulimit -s -H unlimited

        sed -e '/cpython/d' -i ../gcc/testsuite/gcc.dg/plugin/plugin.exp
        sed -e 's/no-pic /&-no-pie /' -i ../gcc/testsuite/gcc.target/i386/pr113689-1.c
        sed -e 's/300000/(1|300000)/' -i ../libgomp/testsuite/libgomp.c-c++-common/pr109062.c
        sed -e 's/{{ target nonpic }} //' \
            -e '/GOTPCREL/d' \
            -i ../gcc/testsuite/gcc.target/i386/fentryname3.c

        make install

        chown -v -R root:root \
            /usr/lib/gcc/$(gcc -dumpmachine)/15.2.0/include{{,-fixed}}

        ln -svr /usr/bin/cpp /usr/lib
        ln -sv gcc.1 /usr/share/man/man1/cc.1
        ln -sfv ../../libexec/gcc/$(gcc -dumpmachine)/15.2.0/liblto_plugin.so \
                /usr/lib/bfd-plugins/

        echo 'int main(){{}}' > dummy.c
        cc dummy.c -v -Wl,--verbose &> dummy.log
        readelf -l a.out | grep ': /lib'

        grep -E -o '/usr/lib.*/S?crt[1in].*succeeded' dummy.log
        grep -B4 '^ /usr/include' dummy.log
        grep 'SEARCH.*/usr/lib' dummy.log |sed 's|; |\\n|g'
        grep \"/lib.*/libc.so.6 \" dummy.log
        grep found dummy.log

        rm -v dummy.c a.out dummy.log

        mkdir -pv /usr/share/gdb/auto-load/usr/lib
        mv -v /usr/lib/*gdb.py /usr/share/gdb/auto-load/usr/lib

        popd

        rm -rf $VORPAL_SOURCE/gcc-pass-03

        ## Build openssl

        mkdir -pv $VORPAL_SOURCE/openssl/openssl-{openssl_version}/build
        pushd $VORPAL_SOURCE/openssl/openssl-{openssl_version}/build

        ../config \
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

        rm -rf $VORPAL_SOURCE/openssl",
    }
}
