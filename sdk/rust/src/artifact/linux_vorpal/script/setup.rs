use indoc::formatdoc;

#[allow(clippy::too_many_arguments)]
pub fn script(
    binutils_version: &str,
    gawk_version: &str,
    gcc_version: &str,
    glibc_version: &str,
    gmp_version: &str,
    mpc_version: &str,
    mpfr_version: &str,
    ncurses_version: &str,
) -> String {
    formatdoc! {"
        set +h
        umask 022

        ### Setup environment

        export VORPAL_SOURCE=\"$(pwd)/source\"

        ### Setup GCC (base)

        pushd $VORPAL_SOURCE/gcc/gcc-{gcc_version}

        mv -v $VORPAL_SOURCE/mpfr/mpfr-{mpfr_version} mpfr
        mv -v $VORPAL_SOURCE/gmp/gmp-{gmp_version} gmp
        mv -v $VORPAL_SOURCE/mpc/mpc-{mpc_version} mpc

        case $(uname -m) in
            x86_64)
                sed -e '/m64=/s/lib64/lib/' \
                    -i.orig gcc/config/i386/t-linux64
            ;;
            aarch64)
                sed -e '/lp64=/s/lib64/lib/' \
                    -i.orig ./gcc/config/aarch64/t-aarch64-linux
            ;;
        esac

        popd

        ## Setup ncurses

        pushd $VORPAL_SOURCE/ncurses/ncurses-{ncurses_version}

        sed -i s/mawk// configure

        popd

        ## Setup gawk 

        pushd $VORPAL_SOURCE/gawk/gawk-{gawk_version}

        sed -i 's/extras//' Makefile.in

        popd

        ## Patch GLIBC

        pushd $VORPAL_SOURCE/glibc/glibc-{glibc_version}

        patch -Np1 -i $VORPAL_SOURCE/glibc-patch/glibc-2.42-fhs-1.patch

        popd

        ## Setup source paths

        mv -v $VORPAL_SOURCE/binutils $VORPAL_SOURCE/binutils-pass-01
        mv -v $VORPAL_SOURCE/glibc $VORPAL_SOURCE/glibc-pass-01
        mv -v $VORPAL_SOURCE/gcc $VORPAL_SOURCE/gcc-pass-01

        echo \"Copying binutils-pass-01 to binutils-pass-02\"
        cp -pr $VORPAL_SOURCE/binutils-pass-01 $VORPAL_SOURCE/binutils-pass-02

        echo \"Copying binutils-pass-02 to binutils-pass-03\"
        cp -pr $VORPAL_SOURCE/binutils-pass-02 $VORPAL_SOURCE/binutils-pass-03

        echo \"Copying gcc-pass-01 to gcc-pass-02\"
        cp -pr $VORPAL_SOURCE/gcc-pass-01 $VORPAL_SOURCE/gcc-pass-02

        echo \"Copying gcc-pass-02 to gcc-pass-03\"
        cp -pr $VORPAL_SOURCE/gcc-pass-02 $VORPAL_SOURCE/gcc-pass-03

        echo \"Copying glibc-pass-01 to glibc-pass-02\"
        cp -pr $VORPAL_SOURCE/glibc-pass-01 $VORPAL_SOURCE/glibc-pass-02

        echo \"Copying gcc-pass-01 to libstdc++\"
        cp -pr $VORPAL_SOURCE/gcc-pass-01 $VORPAL_SOURCE/libstdc++

        ## Patch binutils-pass-02

        pushd $VORPAL_SOURCE/binutils-pass-02/binutils-{binutils_version}

        sed '6009s/$add_dir//' -i ltmain.sh

        popd

        ## Patch gcc-pass-02

        pushd $VORPAL_SOURCE/gcc-pass-02/gcc-{gcc_version}

        sed '/thread_header =/s/@.*@/gthr-posix.h/' \
            -i libgcc/Makefile.in libstdc++-v3/include/Makefile.in

        popd

        ### Setup paths

        mkdir -pv $VORPAL_OUTPUT/{{etc,var}} $VORPAL_OUTPUT/usr/{{bin,lib,sbin}}

        for i in bin lib sbin; do
          ln -sv usr/$i $VORPAL_OUTPUT/$i
        done

        case $(uname -m) in
          aarch64) mkdir -pv $VORPAL_OUTPUT/lib64 ;;
          x86_64) mkdir -pv $VORPAL_OUTPUT/lib64 ;;
        esac

        mkdir -pv $VORPAL_OUTPUT/tools",
    }
}
