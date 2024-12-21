use crate::config::ArtifactSource;

pub fn curl(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://curl.se/download/curl-{version}.tar.xz"),
    }
}

pub fn curl_cacert(hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://curl.se/ca/cacert.pem"),
    }
}

pub fn file(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://astron.com/pub/file/file-{version}.tar.gz"),
    }
}

pub fn gnu(name: &str, version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://ftpmirror.gnu.org/gnu/{name}/{name}-{version}.tar.gz"),
    }
}

pub fn gnu_xz(name: &str, version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://ftpmirror.gnu.org/gnu/{name}/{name}-{version}.tar.xz"),
    }
}

pub fn gnu_gcc(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://ftpmirror.gnu.org/gnu/gcc/gcc-{version}/gcc-{version}.tar.xz"),
    }
}

pub fn gnu_glibc_patch(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!(
            "https://www.linuxfromscratch.org/patches/lfs/12.2/glibc-{version}-fhs-1.patch",
        ),
    }
}

pub fn libidn2(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://ftpmirror.gnu.org/gnu/libidn/libidn2-{version}.tar.gz"),
    }
}

pub fn libpsl(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!(
            "https://github.com/rockdaboot/libpsl/releases/download/{version}/libpsl-{version}.tar.gz",
        ),
    }
}

pub fn linux(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-{version}.tar.xz"),
    }
}

pub fn ncurses(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://invisible-mirror.net/archives/ncurses/ncurses-{version}.tar.gz"),
    }
}

pub fn openssl(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://www.openssl.org/source/openssl-{version}.tar.gz"),
    }
}

pub fn perl(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://www.cpan.org/src/5.0/perl-{version}.tar.xz"),
    }
}

pub fn python(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://www.python.org/ftp/python/{version}/Python-{version}.tar.xz"),
    }
}

pub fn unzip_patch_fixes(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://www.linuxfromscratch.org/patches/blfs/12.2/unzip-{version}-consolidated_fixes-1.patch"),
    }
}

pub fn unzip_patch_gcc14(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!(
            "https://www.linuxfromscratch.org/patches/blfs/12.2/unzip-{version}-gcc14-1.patch"
        ),
    }
}

pub fn unzip(version: &str, hash: &str) -> ArtifactSource {
    let version = version.replace(".", "");

    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://cytranet-dal.dl.sourceforge.net/project/infozip/UnZip 6.x (latest)/UnZip 6.0/unzip{version}.tar.gz?viasf=1",),
    }
}

pub fn util_linux(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!(
            "https://www.kernel.org/pub/linux/utils/util-linux/v2.40/util-linux-{version}.tar.xz"
        ),
    }
}

pub fn xz(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://github.com/tukaani-project/xz/releases/download/v{version}/xz-{version}.tar.xz"),
    }
}

pub fn zlib(version: &str, hash: &str) -> ArtifactSource {
    ArtifactSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        path: format!("https://zlib.net/fossils/zlib-{version}.tar.gz"),
    }
}
