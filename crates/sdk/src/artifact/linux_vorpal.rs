use crate::artifact::linux_debian;
use crate::{
    artifact::{step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::artifact::v0::{
    ArtifactSource,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

pub fn curl(version: &str, hash: &str) -> ArtifactSource {
    let name = "curl";
    let path = format!("https://curl.se/download/{name}-{version}.tar.xz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn curl_cacert(hash: &str) -> ArtifactSource {
    let name = "curl-cacert";
    let path = "https://curl.se/ca/cacert.pem".to_string();

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn file(version: &str, hash: &str) -> ArtifactSource {
    let name = "file";
    let path = format!("https://astron.com/pub/file/file-{version}.tar.gz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn gnu(name: &str, version: &str, hash: &str) -> ArtifactSource {
    let path = format!("https://ftpmirror.gnu.org/gnu/{name}/{name}-{version}.tar.gz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn gnu_xz(name: &str, version: &str, hash: &str) -> ArtifactSource {
    let path = format!("https://ftpmirror.gnu.org/gnu/{name}/{name}-{version}.tar.xz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn gnu_gcc(version: &str, hash: &str) -> ArtifactSource {
    let name = "gcc";
    let path = format!("https://ftpmirror.gnu.org/gnu/gcc/gcc-{version}/gcc-{version}.tar.xz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn gnu_glibc_patch(version: &str, hash: &str) -> ArtifactSource {
    let name = "glibc-patch";
    let path =
        format!("https://www.linuxfromscratch.org/patches/lfs/12.2/glibc-{version}-fhs-1.patch",);

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn libidn2(version: &str, hash: &str) -> ArtifactSource {
    let name = "libidn2";
    let path = format!("https://ftpmirror.gnu.org/gnu/libidn/libidn2-{version}.tar.gz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn libpsl(version: &str, hash: &str) -> ArtifactSource {
    let name = "libpsl";
    let path = format!(
        "https://github.com/rockdaboot/libpsl/releases/download/{version}/libpsl-{version}.tar.gz",
    );

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn linux(version: &str, hash: &str) -> ArtifactSource {
    let name = "linux";
    let path = format!("https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-{version}.tar.xz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn ncurses(version: &str, hash: &str) -> ArtifactSource {
    let name = "ncurses";
    let path = format!("https://invisible-mirror.net/archives/ncurses/ncurses-{version}.tar.gz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn openssl(version: &str, hash: &str) -> ArtifactSource {
    let name = "openssl";
    let path = format!("https://www.openssl.org/source/openssl-{version}.tar.gz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn perl(version: &str, hash: &str) -> ArtifactSource {
    let name = "perl";
    let path = format!("https://www.cpan.org/src/5.0/perl-{version}.tar.xz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn python(version: &str, hash: &str) -> ArtifactSource {
    let name = "python";
    let path = format!("https://www.python.org/ftp/python/{version}/Python-{version}.tar.xz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn unzip_patch_fixes(version: &str, hash: &str) -> ArtifactSource {
    let name = "unzip-patch-fixes";
    let path = format!("https://www.linuxfromscratch.org/patches/blfs/12.2/unzip-{version}-consolidated_fixes-1.patch");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn unzip_patch_gcc14(version: &str, hash: &str) -> ArtifactSource {
    let name = "unzip-patch-gcc14";
    let path =
        format!("https://www.linuxfromscratch.org/patches/blfs/12.2/unzip-{version}-gcc14-1.patch");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn unzip(version: &str, hash: &str) -> ArtifactSource {
    let name = "unzip";
    let version = version.replace(".", "");
    let path = format!("https://cfhcable.dl.sourceforge.net/project/infozip/UnZip%206.x%20%28latest%29/UnZip%206.0/unzip{version}.tar.gz?viasf=1");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn util_linux(version: &str, hash: &str) -> ArtifactSource {
    let name = "util-linux";
    let path = format!(
        "https://www.kernel.org/pub/linux/utils/util-linux/v2.40/util-linux-{version}.tar.xz"
    );

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn xz(version: &str, hash: &str) -> ArtifactSource {
    let name = "xz";
    let path = format!(
        "https://github.com/tukaani-project/xz/releases/download/v{version}/xz-{version}.tar.xz"
    );

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub fn zlib(version: &str, hash: &str) -> ArtifactSource {
    let name = "zlib";
    let path = format!("https://zlib.net/fossils/zlib-{version}.tar.gz");

    ArtifactSourceBuilder::new(name.to_string(), path)
        .with_hash(hash.to_string())
        .build()
}

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let bash_version = "5.2.32";
    let bash = gnu(
        "bash",
        bash_version,
        "19a8087c947a587b491508a6675a5349e23992d5dfca40a0bd0735bbd81e0438",
    );

    let binutils_version = "2.43.1";
    let binutils = gnu(
        "binutils",
        binutils_version,
        "c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528",
    );

    let bison_version = "3.8.2";
    let bison = gnu(
        "bison",
        bison_version,
        "cb18c2c8562fc01bf3ae17ffe9cf8274e3dd49d39f89397c1a8bac7ee14ce85f",
    );

    let coreutils_version = "9.5";
    let coreutils = gnu(
        "coreutils",
        coreutils_version,
        "af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4",
    );

    let curl_version = "8.11.0";
    let curl = curl(
        curl_version,
        "97dde4e45e89291bf5405b0363b16049333366f286a1989537441c261e9299fe",
    );

    let curl_cacert =
        curl_cacert("74e20ed700e895a3b5f58dbcad2b20f98f041e167d50686cda66b6337af6aa21");

    let diffutils_version = "3.10";
    let diffutils = gnu_xz(
        "diffutils",
        diffutils_version,
        "5045e29e7fa0ffe017f63da7741c800cbc0f89e04aebd78efcd661d6e5673326",
    );

    let file_version = "5.45";
    let file = file(
        file_version,
        "c118ab56efa05798022a5a488827594a82d844f65159e95b918d5501adf1e58f",
    );

    let findutils_version = "4.10.0";
    let findutils = gnu_xz(
        "findutils",
        findutils_version,
        "242f804d87a5036bb0fab99966227dc61e853e5a67e1b10c3cc45681c792657e",
    );

    let gawk_version = "5.3.0";
    let gawk = gnu(
        "gawk",
        gawk_version,
        "a21e5899707ddc030a0fcc0a35c95a9602dca1a681fa52a1790a974509b40133",
    );

    let gcc_version = "14.2.0";
    let gcc = gnu_gcc(
        gcc_version,
        "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583",
    );

    let gettext_version = "0.22.5";
    let gettext = gnu(
        "gettext",
        gettext_version,
        "6e3ef842d1006a6af7778a8549a8e8048fc3b923e5cf48eaa5b82b5d142220ae",
    );

    let glibc_version = "2.40";
    let glibc = gnu(
        "glibc",
        glibc_version,
        "da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b",
    );

    let glibc_patch = gnu_glibc_patch(
        glibc_version,
        "69cf0653ad0a6a178366d291f30629d4e1cb633178aa4b8efbea0c851fb944ca",
    );

    let grep_version = "3.11";
    let grep = gnu(
        "grep",
        grep_version,
        "1625eae01f6e4dbc41b58545aa2326c74791b2010434f8241d41903a4ea5ff70",
    );

    let gzip_version = "1.13";
    let gzip = gnu(
        "gzip",
        gzip_version,
        "25e51d46402bab819045d452ded6c4558ef980f5249c470d9499e9eae34b59b1",
    );

    let libidn2_version = "2.3.7";
    let libidn2 = libidn2(
        libidn2_version,
        "cb09b889bc9e51a2f5ec9d04dbbf03582926a129340828271955d15a57da6a3c",
    );

    let libpsl_version = "0.21.5";
    let libpsl = libpsl(
        libpsl_version,
        "65ecfe61646c50119a018a2003149833c11387efd92462f974f1ff9f907c1d78",
    );

    let libunistring_version = "1.2";
    let libunistring = gnu(
        "libunistring",
        libunistring_version,
        "c621c94a94108095cfe08cc61f484d4b4cb97824c64a4e2bb1830d8984b542f3",
    );

    let linux_version = "6.10.5";
    let linux = linux(
        linux_version,
        "b1548c4f5bf63c5f44c1a8c3044842a49ef445deb1b3da55b8116200a25793be",
    );

    let m4_version = "1.4.19";
    let m4 = gnu(
        "m4",
        m4_version,
        "fd793cdfc421fac76f4af23c7d960cbe4a29cbb18f5badf37b85e16a894b3b6d",
    );

    let make_version = "4.4.1";
    let make = gnu(
        "make",
        make_version,
        "8dfe7b0e51b3e190cd75e046880855ac1be76cf36961e5cfcc82bfa91b2c3ba8",
    );

    let ncurses_version = "6.5";
    let ncurses = ncurses(
        ncurses_version,
        "aab234a3b7a22e2632151fbe550cb36e371d3ee5318a633ee43af057f9f112fb",
    );

    let openssl_version = "3.3.1";
    let openssl = openssl(
        openssl_version,
        "a53e2254e36124452582477935a680f07f9884fe1d6e9ec03c28ac71b750d84a",
    );

    let patch_version = "2.7.6";
    let patch = gnu(
        "patch",
        "2.7.6",
        "af8c281a05a6802075799c0c179e5fb3a218be6a21b726d8b672cd0f4c37eae9",
    );

    let perl_version = "5.40.0";
    let perl = perl(
        perl_version,
        "59b6437a3da1d9de0126135b31f1f16aee9c3b7a0f61f6364b2da3e8bb5f771f",
    );

    let python_version = "3.12.5";
    let python = python(
        python_version,
        "8359773924d33702ecd6f9fab01973e53d929d46d7cdc4b0df31eb1282c68b67",
    );

    let sed_version = "4.9";
    let sed = gnu(
        "sed",
        sed_version,
        "434ff552af89340088e0d8cb206c251761297909bbee401176bc8f655e8e7cf2",
    );

    let tar_version = "1.35";
    let tar = gnu(
        "tar",
        tar_version,
        "f9bb5f39ed45b1c6a324470515d2ef73e74422c5f345503106d861576d3f02f3",
    );

    let texinfo_version = "7.1.1";
    let texinfo = gnu(
        "texinfo",
        texinfo_version,
        "6e34604552af91db0b4ccf0bcceba63dd3073da2a492ebcf33c6e188a64d2b63",
    );

    let unzip_version = "6.0";
    let unzip = unzip(
        unzip_version,
        "4585067be297ae977da3f81587fcf0a141a8d6ceb6137d199255683ed189c3ed",
    );

    let unzip_patch_fixes = unzip_patch_fixes(
        "6.0",
        "11350935be5bbb743f1a97ec069b78fc2904f92b24abbc7fb3d7f0ff8bb889ea",
    );

    let unzip_patch_gcc14 = unzip_patch_gcc14(
        "6.0",
        "d6ac941672086ea4c8d5047d550b40047825a685cc7c48626d2f0939e1a0c797",
    );

    let util_linux_version = "2.40.2";
    let util_linux = util_linux(
        util_linux_version,
        "7db19a1819ac5c743b52887a4571e42325b2bfded63d93b6a1797ae2b1f8019a",
    );

    let xz_version = "5.6.2";
    let xz = xz(
        xz_version,
        "7a02b1278ed9a59b332657d613c5549b39afe34e315197f4da95c5322524ec26",
    );

    let zlib_version = "1.3.1";
    let zlib = zlib(
        zlib_version,
        "3f7995d5f103719283f509c23624287ce95c349439e881ed935a3c2c807bb683",
    );

    let environments = vec!["PATH=/usr/bin:/usr/sbin".to_string()];

    let rootfs = linux_debian::build(context).await?;

    let step_setup = step::bwrap(
        context,
        vec![],
        vec![],
        environments.clone(),
        Some(rootfs.clone()),
        formatdoc! {"
            set +h
            umask 022

            ### Setup environment

            export VORPAL_SOURCE=\"$(pwd)/source\"

            ### Setup GCC (base)

            pushd $VORPAL_SOURCE/gcc/gcc-{gcc_version}

            ./contrib/download_prerequisites

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

            patch -Np1 -i $VORPAL_SOURCE/glibc-patch/glibc-{glibc_version}-fhs-1.patch

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
        },
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    let step_stage_01 = step::bwrap(
        context,
        vec![],
        vec![],
        environments.clone(),
        Some(rootfs.clone()),
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
        },
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    let step_stage_02 = step::bwrap(
        context,
        vec![],
        vec![],
        environments.clone(),
        Some(rootfs.clone()),
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
        },
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    // TODO: impove readability with list in list

    let arguments = vec![
        // mount bin
        "--bind".to_string(),
        "$VORPAL_OUTPUT/bin".to_string(),
        "/bin".to_string(),
        // mount etc
        "--bind".to_string(),
        "$VORPAL_OUTPUT/etc".to_string(),
        "/etc".to_string(),
        // mount lib
        "--bind".to_string(),
        "$VORPAL_OUTPUT/lib".to_string(),
        "/lib".to_string(),
        // mount lib64 (if exists)
        "--bind-try".to_string(),
        "$VORPAL_OUTPUT/lib64".to_string(),
        "/lib64".to_string(),
        // mount sbin
        "--bind".to_string(),
        "$VORPAL_OUTPUT/sbin".to_string(),
        "/sbin".to_string(),
        // mount usr
        "--bind".to_string(),
        "$VORPAL_OUTPUT/usr".to_string(),
        "/usr".to_string(),
        // mount current directory
        "--bind".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        // change directory
        "--chdir".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        // set group id
        "--gid".to_string(),
        "0".to_string(),
        // set user id
        "--uid".to_string(),
        "0".to_string(),
    ];

    let step_stage_03 = step::bwrap(
        context,
        [
            arguments.clone(),
            vec![
                // mount tools
                "--bind".to_string(),
                "$VORPAL_OUTPUT/tools".to_string(),
                "/tools".to_string(),
            ],
        ]
        .concat(),
        vec![],
        environments.clone(),
        None,
        formatdoc! {"
            ## Setup paths

            export VORPAL_SOURCE=\"$(pwd)/source\"

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

            mkdir -pv $VORPAL_SOURCE/gettext/gettext-{gettext_version}/build
            pushd $VORPAL_SOURCE/gettext/gettext-{gettext_version}/build

            ../configure --disable-shared

            make

            cp -pv gettext-tools/src/{{msgfmt,msgmerge,xgettext}} /usr/bin

            popd

            rm -rf $VORPAL_SOURCE/gettext

            ## Build bison

            mkdir -pv $VORPAL_SOURCE/bison/bison-{bison_version}/build
            pushd $VORPAL_SOURCE/bison/bison-{bison_version}/build

            ../configure \
                --prefix=\"/usr\" \
                --docdir=\"/usr/share/doc/bison-3.8.2\"

            make
            make install

            popd

            rm -rf $VORPAL_SOURCE/bison

            ## Build perl

            pushd $VORPAL_SOURCE/perl/perl-{perl_version}

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

            rm -rf $VORPAL_SOURCE/perl

            ## Build Python

            mkdir -pv $VORPAL_SOURCE/python/Python-{python_version}/build
            pushd $VORPAL_SOURCE/python/Python-{python_version}/build

            ../configure \
                --prefix=\"/usr\" \
                --enable-shared \
                --without-ensurepip

            make
            make install

            popd

            rm -rf $VORPAL_SOURCE/python

            ## Build texinfo

            mkdir -pv $VORPAL_SOURCE/texinfo/texinfo-{texinfo_version}/build
            pushd $VORPAL_SOURCE/texinfo/texinfo-{texinfo_version}/build

            ../configure --prefix=\"/usr\"

            make
            make install

            popd

            rm -rf $VORPAL_SOURCE/texinfo

            ## Build util-linux

            mkdir -pv $VORPAL_SOURCE/util-linux/util-linux-{util_linux_version}/build
            pushd $VORPAL_SOURCE/util-linux/util-linux-{util_linux_version}/build

            mkdir -pv /var/lib/hwclock

            # note: \"--disable-makeinstall-chown\" for sandbox limitations

            ../configure \
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

            rm -rf $VORPAL_SOURCE/util-linux

            ## Cleanup

            rm -rf /usr/share/{{info,man,doc}}/*

            find /usr/{{lib,libexec}} -name \\*.la -delete",
        },
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    let step_cleanup = step::bwrap(
        context,
        vec![],
        vec![],
        environments.clone(),
        Some(rootfs.clone()),
        formatdoc! {"
            rm -rf $VORPAL_OUTPUT/tools",
        },
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    let step_stage_04 = step::bwrap(
        context,
        arguments.clone(),
        vec![],
        environments.clone(),
        None,
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
                /usr/lib/gcc/$(gcc -dumpmachine)/14.2.0/include{{,-fixed}}

            ln -svr /usr/bin/cpp /usr/lib
            ln -sv gcc.1 /usr/share/man/man1/cc.1
            ln -sfv ../../libexec/gcc/$(gcc -dumpmachine)/14.2.0/liblto_plugin.so \
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
        },
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    let step_stage_05 = step::bwrap(
        context,
        arguments.clone(),
        vec![],
        environments.clone(),
        None,
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
        },
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    let name = "linux-vorpal";

    ArtifactBuilder::new(name.to_string())
        .with_source(bash)
        .with_source(binutils)
        .with_source(bison)
        .with_source(coreutils)
        .with_source(curl)
        .with_source(curl_cacert)
        .with_source(diffutils)
        .with_source(file)
        .with_source(findutils)
        .with_source(gawk)
        .with_source(gcc)
        .with_source(gettext)
        .with_source(glibc)
        .with_source(glibc_patch)
        .with_source(grep)
        .with_source(gzip)
        .with_source(libidn2)
        .with_source(libpsl)
        .with_source(libunistring)
        .with_source(linux)
        .with_source(m4)
        .with_source(make)
        .with_source(ncurses)
        .with_source(openssl)
        .with_source(patch)
        .with_source(perl)
        .with_source(python)
        .with_source(sed)
        .with_source(tar)
        .with_source(texinfo)
        .with_source(unzip)
        .with_source(unzip_patch_fixes)
        .with_source(unzip_patch_gcc14)
        .with_source(util_linux)
        .with_source(xz)
        .with_source(zlib)
        .with_step(step_setup)
        .with_step(step_stage_01)
        .with_step(step_stage_02)
        .with_step(step_stage_03)
        .with_step(step_cleanup)
        .with_step(step_stage_04)
        .with_step(step_stage_05)
        .with_system(Aarch64Linux)
        .with_system(X8664Linux)
        .build(context)
        .await
}
