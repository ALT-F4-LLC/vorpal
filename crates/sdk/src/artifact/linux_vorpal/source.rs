use crate::artifact::ArtifactSourceBuilder;
use vorpal_schema::artifact::v0::ArtifactSource;

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
