use crate::{api, artifact::ArtifactSource};

pub fn curl(version: &str) -> api::artifact::ArtifactSource {
    let name = "curl";
    let path = format!("https://sdk.vorpal.build/source/{name}-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn curl_cacert(version: &str) -> api::artifact::ArtifactSource {
    let name = "curl-cacert";
    let path = format!("https://sdk.vorpal.build/source/cacert-{version}.pem");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn file(version: &str) -> api::artifact::ArtifactSource {
    let name = "file";
    let path = format!("https://sdk.vorpal.build/source/file-{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn gnu(name: &str, version: &str) -> api::artifact::ArtifactSource {
    let path = format!("https://sdk.vorpal.build/source/{name}-{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn gnu_xz(name: &str, version: &str) -> api::artifact::ArtifactSource {
    let path = format!("https://sdk.vorpal.build/source/{name}-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn gnu_gcc(version: &str) -> api::artifact::ArtifactSource {
    let name = "gcc";
    let path = format!("https://sdk.vorpal.build/source/gcc-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn gnu_glibc_patch(version: &str) -> api::artifact::ArtifactSource {
    let name = "glibc-patch";
    let path = format!("https://sdk.vorpal.build/source/glibc-{version}-fhs-1.patch");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn libidn2(version: &str) -> api::artifact::ArtifactSource {
    let name = "libidn2";
    let path = format!("https://sdk.vorpal.build/source/libidn2-{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn libpsl(version: &str) -> api::artifact::ArtifactSource {
    let name = "libpsl";
    let path = format!("https://sdk.vorpal.build/source/libpsl-{version}.tar.gz",);

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn linux(version: &str) -> api::artifact::ArtifactSource {
    let name = "linux";
    let path = format!("https://sdk.vorpal.build/source/linux-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn ncurses(version: &str) -> api::artifact::ArtifactSource {
    let name = "ncurses";
    let path = format!("https://sdk.vorpal.build/source/ncurses-{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn openssl(version: &str) -> api::artifact::ArtifactSource {
    let name = "openssl";
    let path = format!("https://sdk.vorpal.build/source/openssl-{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn perl(version: &str) -> api::artifact::ArtifactSource {
    let name = "perl";
    let path = format!("https://sdk.vorpal.build/source/perl-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn python(version: &str) -> api::artifact::ArtifactSource {
    let name = "python";
    let path = format!("https://sdk.vorpal.build/source/Python-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn unzip_patch_fixes(version: &str) -> api::artifact::ArtifactSource {
    let name = "unzip-patch-fixes";
    let path =
        format!("https://sdk.vorpal.build/source/unzip-{version}-consolidated_fixes-1.patch");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn unzip_patch_gcc14(version: &str) -> api::artifact::ArtifactSource {
    let name = "unzip-patch-gcc14";
    let path = format!("https://sdk.vorpal.build/source/unzip-{version}-gcc14-1.patch");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn unzip(version: &str) -> api::artifact::ArtifactSource {
    let name = "unzip";
    let version = version.replace(".", "");
    let path = format!("https://sdk.vorpal.build/source/unzip{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn util_linux(version: &str) -> api::artifact::ArtifactSource {
    let name = "util-linux";
    let path = format!("https://sdk.vorpal.build/source/util-linux-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn xz(version: &str) -> api::artifact::ArtifactSource {
    let name = "xz";
    let path = format!("https://sdk.vorpal.build/source/xz-{version}.tar.xz");

    ArtifactSource::new(name, path.as_str()).build()
}

pub fn zlib(version: &str) -> api::artifact::ArtifactSource {
    let name = "zlib";
    let path = format!("https://sdk.vorpal.build/source/zlib-{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}
