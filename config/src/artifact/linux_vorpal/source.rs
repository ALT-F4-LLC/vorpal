use crate::{
    artifact::{run_bwrap_step, step_env_artifact},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

fn new_source_name(name: &str) -> String {
    format!("linux-vorpal-{}-source", name)
}

fn new_source(
    artifacts: Vec<ArtifactId>,
    name: String,
    rootfs: &ArtifactId,
    script: String,
) -> Artifact {
    let artifacts = vec![artifacts.clone(), vec![rootfs.clone()]].concat();

    Artifact {
        artifacts: artifacts.clone(),
        name,
        sources: vec![],
        steps: vec![run_bwrap_step(
            vec![],
            artifacts,
            vec![ArtifactEnvironment {
                key: "PATH".to_string(),
                value: "/usr/bin:/usr/sbin".to_string(),
            }],
            Some(step_env_artifact(rootfs)),
            script,
        )],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    }
}

pub fn bash(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("bash"),
        rootfs,
        formatdoc! {"
            curl -L -o ./bash-{version}.tar.gz \
                https://ftpmirror.gnu.org/gnu/bash/bash-{version}.tar.gz

            tar -xvf ./bash-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
            version = "5.2.32",
        },
    ))
}

pub fn binutils(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("binutils"),
        rootfs,
        formatdoc! {"
            curl -L -o ./binutils-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/binutils/binutils-{version}.tar.xz

            tar -xvf ./binutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "2.43.1",
        },
    ))
}

pub fn bison(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("bison"),
        rootfs,
        formatdoc! {"
            curl -L -o ./bison-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/bison/bison-{version}.tar.xz

            tar -xvf ./bison-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "3.8.2",
        },
    ))
}

pub fn coreutils(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("coreutils"),
        rootfs,
        formatdoc! {"
            curl -L -o ./coreutils-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/coreutils/coreutils-{version}.tar.xz

            tar -xvf ./coreutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "9.5",
        },
    ))
}

pub fn curl(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("curl"),
        rootfs,
        formatdoc! {"
            curl -L -o ./curl-{version}.tar.xz \
                https://curl.se/download/curl-{version}.tar.xz

            tar -xvf ./curl-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "8.11.0",
        },
    ))
}

pub fn diffutils(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("diffutils"),
        rootfs,
        formatdoc! {"
            curl -L -o ./diffutils-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/diffutils/diffutils-{version}.tar.xz

            tar -xvf ./diffutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "3.10",
        },
    ))
}

pub fn file(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("file"),
        rootfs,
        formatdoc! {"
            curl -L -o ./file-{version}.tar.gz \
                https://astron.com/pub/file/file-{version}.tar.gz

            tar -xvf ./file-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
            version = "5.45",
        },
    ))
}

pub fn findutils(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("findutils"),
        rootfs,
        formatdoc! {"
            curl -L -o ./findutils-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/findutils/findutils-{version}.tar.xz

            tar -xvf ./findutils-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "4.10.0",
        },
    ))
}

pub fn gawk(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("gawk"),
        rootfs,
        formatdoc! {"
            curl -L -o ./gawk-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/gawk/gawk-{version}.tar.xz

            tar -xvf ./gawk-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1

            sed -i 's/extras//' $VORPAL_OUTPUT/Makefile.in",
            version = "5.3.0",
        },
    ))
}

pub fn gcc(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("gcc"),
        rootfs,
        formatdoc! {"
            curl -L -o ./gcc-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/gcc/gcc-{version}/gcc-{version}.tar.xz

            tar -xvf ./gcc-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1

            pushd $VORPAL_OUTPUT

            ./contrib/download_prerequisites

            sed -e '/lp64=/s/lib64/lib/' \
                -i.orig $VORPAL_OUTPUT/gcc/config/aarch64/t-aarch64-linux

            sed -e '/m64=/s/lib64/lib/' \
                -i.orig $VORPAL_OUTPUT/gcc/config/i386/t-linux64",
            version = "14.2.0",
        },
    ))
}

pub fn gettext(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("gettext"),
        rootfs,
        formatdoc! {"
            curl -L -o ./gettext-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/gettext/gettext-{version}.tar.xz

            tar -xvf ./gettext-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "0.22.5",
        },
    ))
}

pub fn glibc_patch(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("glibc-patch"),
        rootfs,
        formatdoc! {"
            curl -L -o ./glibc-patch-{version}.patch \
                https://www.linuxfromscratch.org/patches/lfs/12.2/glibc-{version}-fhs-1.patch

            cp -v ./glibc-patch-{version}.patch $VORPAL_OUTPUT",
            version = "2.40",
        },
    ))
}

pub fn glibc(
    context: &mut ContextConfig,
    glibc_patch: &ArtifactId,
    rootfs: &ArtifactId,
) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![glibc_patch.clone()],
        new_source_name("glibc"),
        rootfs,
        formatdoc! {"
            curl -L -o ./glibc-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/glibc/glibc-{version}.tar.xz

            tar -xvf ./glibc-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1

            pushd $VORPAL_OUTPUT

            patch -Np1 -i {glibc_patch}/glibc-patch-2.40.patch",
            glibc_patch = step_env_artifact(glibc_patch),
            version = "2.40",
        },
    ))
}

pub fn grep(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("grep"),
        rootfs,
        formatdoc! {"
            curl -L -o ./grep-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/grep/grep-{version}.tar.xz

            tar -xvf ./grep-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "3.11",
        },
    ))
}

pub fn gzip(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("gzip"),
        rootfs,
        formatdoc! {"
            curl -L -o ./gzip-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/gzip/gzip-{version}.tar.xz

            tar -xvf ./gzip-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "1.13",
        },
    ))
}

pub fn linux_headers(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("linux-headers"),
        rootfs,
        formatdoc! {"
            curl -L -o ./linux-headers-{version}.tar.xz \
                https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-{version}.tar.xz

            tar -xvf ./linux-headers-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "6.10.5",
        },
    ))
}

pub fn m4(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("m4"),
        rootfs,
        formatdoc! {"
            curl -L -o ./m4-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/m4/m4-{version}.tar.xz

            tar -xvf ./m4-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "1.4.19",
        },
    ))
}

pub fn make(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("make"),
        rootfs,
        formatdoc! {"
            curl -L -o ./make-{version}.tar.gz \
                https://ftpmirror.gnu.org/gnu/make/make-{version}.tar.gz

            tar -xvf ./make-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
            version = "4.4.1",
        },
    ))
}

pub fn ncurses(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("ncurses"),
        rootfs,
        formatdoc! {"
            curl -L -o ./ncurses-{version}.tar.gz \
                https://invisible-mirror.net/archives/ncurses/ncurses-{version}.tar.gz

            tar -xvf ./ncurses-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
            version = "6.5",
        },
    ))
}

pub fn patch(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("patch"),
        rootfs,
        formatdoc! {"
            curl -L -o ./patch-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/patch/patch-{version}.tar.xz

            tar -xvf ./patch-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "2.7.6",
        },
    ))
}

pub fn perl(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("perl"),
        rootfs,
        formatdoc! {"
            curl -L -o ./perl-{version}.tar.gz https://www.cpan.org/src/5.0/perl-{version}.tar.xz

            tar -xvf ./perl-{version}.tar.gz -C $VORPAL_OUTPUT --strip-components=1",
            version = "5.40.0",
        },
    ))
}

pub fn python(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("python"),
        rootfs,
        formatdoc! {"
            curl -L -o ./python-{version}.tar.xz \
                https://www.python.org/ftp/python/{version}/Python-{version}.tar.xz

            tar -xvf ./python-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "3.12.5",
        },
    ))
}

pub fn sed(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("sed"),
        rootfs,
        formatdoc! {"
            curl -L -o ./sed-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/sed/sed-{version}.tar.xz

            tar -xvf ./sed-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "4.9",
        },
    ))
}

pub fn tar(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("tar"),
        rootfs,
        formatdoc! {"
            curl -L -o ./tar-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/tar/tar-{version}.tar.xz

            tar -xvf ./tar-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "1.35",
        },
    ))
}

pub fn texinfo(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("texinfo"),
        rootfs,
        formatdoc! {"
            curl -L -o ./texinfo-{version}.tar.xz \
                https://ftpmirror.gnu.org/gnu/texinfo/texinfo-{version}.tar.xz

            tar -xvf ./texinfo-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "7.1.1",
        },
    ))
}

pub fn util_linux(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("util-linux"),
        rootfs,
        formatdoc! {"
            curl -L -o ./util-linux-{version}.tar.xz \
                https://www.kernel.org/pub/linux/utils/util-linux/v2.40/util-linux-{version}.tar.xz

            tar -xvf ./util-linux-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "2.40.2",
        },
    ))
}

pub fn xz(context: &mut ContextConfig, rootfs: &ArtifactId) -> Result<ArtifactId> {
    context.add_artifact(new_source(
        vec![],
        new_source_name("xz"),
        rootfs,
        formatdoc! {"
            curl -L -o ./xz-{version}.tar.xz \
                https://github.com/tukaani-project/xz/releases/download/v{version}/xz-{version}.tar.xz

            tar -xvf ./xz-{version}.tar.xz -C $VORPAL_OUTPUT --strip-components=1",
            version = "5.6.2",
        },
    ))
}
