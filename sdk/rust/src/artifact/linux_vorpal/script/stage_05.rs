use indoc::formatdoc;

pub fn script(
    curl_version: &str,
    libidn2_version: &str,
    libpsl_version: &str,
    libunistring_version: &str,
    unzip_version: &str,
) -> String {
    formatdoc! {"
        ## Setup environment

        export MAKEFLAGS=\"-j$(nproc)\"
        export VORPAL_SOURCE=\"$(pwd)/source\"
        export VORPAL_TARGET=\"$(uname -m)-vorpal-linux-gnu\"

        ## Build libunistring

        mkdir -pv $VORPAL_SOURCE/libunistring/libunistring-{libunistring_version}/build
        pushd $VORPAL_SOURCE/libunistring/libunistring-{libunistring_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --disable-static \
            --docdir=\"/usr/share/doc/libunistring-1.2\"

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/libunistring

        ## Build libidn2

        mkdir -pv $VORPAL_SOURCE/libidn2/libidn2-{libidn2_version}/build
        pushd $VORPAL_SOURCE/libidn2/libidn2-{libidn2_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --disable-static

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/libidn2

        ## Build libpsl

        mkdir -pv $VORPAL_SOURCE/libpsl/libpsl-{libpsl_version}/build
        pushd $VORPAL_SOURCE/libpsl/libpsl-{libpsl_version}/build

        ../configure --prefix=\"/usr\"

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/libpsl

        ## Build CA certificates

        cp -pv $VORPAL_SOURCE/curl-cacert/cacert.pem /etc/ssl/certs/ca-certificates.crt

        ## Build curl

        mkdir -pv $VORPAL_SOURCE/curl/curl-{curl_version}/build
        pushd $VORPAL_SOURCE/curl/curl-{curl_version}/build

        ../configure \
            --prefix=\"/usr\" \
            --disable-static \
            --with-openssl \
            --enable-threaded-resolver \
            --with-ca-path=\"/etc/ssl/certs\"

        make
        make install

        popd

        rm -rf $VORPAL_SOURCE/curl

        ## Build unzip

        pushd $VORPAL_SOURCE/unzip/unzip{unzip_version}

        patch -Np1 -i $VORPAL_SOURCE/unzip-patch-fixes/unzip-6.0-consolidated_fixes-1.patch
        patch -Np1 -i $VORPAL_SOURCE/unzip-patch-gcc14/unzip-6.0-gcc14-1.patch

        make -f unix/Makefile generic

        make prefix=/usr MANDIR=/usr/share/man/man1 \
            -f unix/Makefile install

        popd

        rm -rf $VORPAL_SOURCE/unzip

        ## Cleanup

        find /usr/lib /usr/libexec -name \\*.la -delete

        find /usr -depth -name $VORPAL_TARGET\\* | xargs rm -rf",
        unzip_version = unzip_version.replace(".", "").as_str(),
    }
}
