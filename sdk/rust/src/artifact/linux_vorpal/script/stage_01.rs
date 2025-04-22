use indoc::formatdoc;

pub fn script(
    binutils_version: &str,
    gcc_version: &str,
    glibc_version: &str,
    linux_version: &str,
) -> String {
    formatdoc! {"
        set +h
        umask 022

        ### Setup environment

        export LC_ALL=\"POSIX\"
        export VORPAL_TARGET=\"$(uname -m)-vorpal-linux-gnu\"
        export PATH=\"$VORPAL_OUTPUT/tools/bin:$PATH\"
        export CONFIG_SITE=\"$VORPAL_OUTPUT/usr/share/config.site\"
        export MAKEFLAGS=\"-j$(nproc)\"
        export VORPAL_SOURCE=\"$(pwd)/source\"

        ### Build binutils (pass 01)

        mkdir -pv $VORPAL_SOURCE/binutils-pass-01/binutils-{binutils_version}/build
        pushd $VORPAL_SOURCE/binutils-pass-01/binutils-{binutils_version}/build

        ../configure \
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

        rm -rf $VORPAL_SOURCE/binutils-pass-01

        ### Build gcc (pass 01)

        mkdir -pv $VORPAL_SOURCE/gcc-pass-01/gcc-{gcc_version}/build
        pushd $VORPAL_SOURCE/gcc-pass-01/gcc-{gcc_version}/build

        ../configure \
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

        cat ../gcc/limitx.h ../gcc/glimits.h ../gcc/limity.h \
            > $OUTPUT_LIMITS_PATH

        popd

        rm -rf $VORPAL_SOURCE/gcc-pass-01

        ### Build linux headers

        pushd $VORPAL_SOURCE/linux/linux-{linux_version}

        make mrproper
        make headers

        find usr/include -type f ! -name '*.h' -delete

        cp -prv usr/include \"$VORPAL_OUTPUT/usr\"

        popd

        rm -rf $VORPAL_SOURCE/linux/linux-{linux_version}

        ### Build glibc-pass-01

        mkdir -pv $VORPAL_SOURCE/glibc-pass-01/glibc-{glibc_version}/build
        pushd $VORPAL_SOURCE/glibc-pass-01/glibc-{glibc_version}/build

        case $(uname -m) in
            aarch64) ln -sfv ../lib/ld-linux-aarch64.so.1 $VORPAL_OUTPUT/lib64
            ;;
            i?86)   ln -sfv ld-linux.so.2 $VORPAL_OUTPUT/lib/ld-lsb.so.3
            ;;
            x86_64) ln -sfv ../lib/ld-linux-x86-64.so.2 $VORPAL_OUTPUT/lib64
                    ln -sfv ../lib/ld-linux-x86-64.so.2 $VORPAL_OUTPUT/lib64/ld-lsb-x86-64.so.3
            ;;
        esac

        echo \"rootsbindir=/usr/sbin\" > configparms

        ../configure \
        --prefix=\"/usr\" \
        --host=\"$VORPAL_TARGET\" \
        --build=\"$(../scripts/config.guess)\" \
        --enable-kernel=\"4.19\" \
        --with-headers=\"$VORPAL_OUTPUT/usr/include\" \
        --disable-nscd \
            libc_cv_slibdir=\"/usr/lib\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        sed '/RTLDLIST=/s@/usr@@g' -i $VORPAL_OUTPUT/usr/bin/ldd

        popd

        rm -rf $VORPAL_SOURCE/glibc-pass-01

        ## Test glibc

        echo 'Testing glibc'
        echo 'int main(){{}}' | $VORPAL_TARGET-gcc -xc -

        readelf -l a.out | grep ld-linux

        rm -v a.out

        ## Build libstdc++

        mkdir -pv $VORPAL_SOURCE/libstdc++/gcc-{gcc_version}/build
        pushd $VORPAL_SOURCE/libstdc++/gcc-{gcc_version}/build

        ../libstdc++-v3/configure \
        --host=\"$VORPAL_TARGET\" \
        --build=\"$(../libstdc++/config.guess)\" \
        --prefix=\"/usr\" \
        --disable-multilib \
        --disable-nls \
        --disable-libstdcxx-pch \
            --with-gxx-include-dir=\"/tools/$VORPAL_TARGET/include/c++/14.2.0\"

        make
        make DESTDIR=\"$VORPAL_OUTPUT\" install

        rm -v $VORPAL_OUTPUT/usr/lib/lib{{stdc++{{,exp,fs}},supc++}}.la

        popd

        rm -rf $VORPAL_SOURCE/libstdc++",
    }
}
