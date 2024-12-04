use crate::config::artifact::{get_artifact_envkey, steps, ContextConfig};
use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

mod script;
mod source;

pub fn artifact(context: &mut ContextConfig, linux_debian: &ArtifactId) -> Result<ArtifactId> {
    let bash = source::bash(context, linux_debian)?;
    let binutils = source::binutils(context, linux_debian)?;
    let bison = source::bison(context, linux_debian)?;
    let coreutils = source::coreutils(context, linux_debian)?;
    let curl = source::curl(context, linux_debian)?;
    let curl_cacert = source::curl_cacert(context, linux_debian)?;
    let diffutils = source::diffutils(context, linux_debian)?;
    let file = source::file(context, linux_debian)?;
    let findutils = source::findutils(context, linux_debian)?;
    let gawk = source::gawk(context, linux_debian)?;
    let gcc = source::gcc(context, linux_debian)?;
    let gettext = source::gettext(context, linux_debian)?;
    let glibc_patch = source::glibc_patch(context, linux_debian)?;
    let glibc = source::glibc(context, &glibc_patch, linux_debian)?;
    let grep = source::grep(context, linux_debian)?;
    let gzip = source::gzip(context, linux_debian)?;
    let libidn2 = source::libidn2(context, linux_debian)?;
    let libpsl = source::libpsl(context, linux_debian)?;
    let libunistring = source::libunistring(context, linux_debian)?;
    let linux_headers = source::linux_headers(context, linux_debian)?;
    let m4 = source::m4(context, linux_debian)?;
    let make = source::make(context, linux_debian)?;
    let ncurses = source::ncurses(context, linux_debian)?;
    let openssl = source::openssl(context, linux_debian)?;
    let patch = source::patch(context, linux_debian)?;
    let perl = source::perl(context, linux_debian)?;
    let python = source::python(context, linux_debian)?;
    let sed = source::sed(context, linux_debian)?;
    let tar = source::tar(context, linux_debian)?;
    let texinfo = source::texinfo(context, linux_debian)?;
    let unzip_patch_fixes = source::unzip_patch_fixes(context, linux_debian)?;
    let unzip_patch_gcc14 = source::unzip_patch_gcc14(context, linux_debian)?;
    let unzip = source::unzip(
        context,
        linux_debian,
        &unzip_patch_fixes,
        &unzip_patch_gcc14,
    )?;
    let util_linux = source::util_linux(context, linux_debian)?;
    let xz = source::xz(context, linux_debian)?;
    let zlib = source::zlib(context, linux_debian)?;

    // TODO: implement "expect_hash" for artifactIds

    let artifacts = vec![
        bash.clone(),
        binutils.clone(),
        bison.clone(),
        coreutils.clone(),
        curl.clone(),
        curl_cacert.clone(),
        diffutils.clone(),
        file.clone(),
        findutils.clone(),
        gawk.clone(),
        gcc.clone(),
        gettext.clone(),
        glibc.clone(),
        grep.clone(),
        gzip.clone(),
        libidn2.clone(),
        libpsl.clone(),
        libunistring.clone(),
        linux_debian.clone(),
        linux_headers.clone(),
        m4.clone(),
        make.clone(),
        ncurses.clone(),
        openssl.clone(),
        patch.clone(),
        perl.clone(),
        python.clone(),
        sed.clone(),
        tar.clone(),
        texinfo.clone(),
        unzip.clone(),
        util_linux.clone(),
        xz.clone(),
        zlib.clone(),
    ];

    // Setup environment

    context.add_artifact(Artifact {
        artifacts: artifacts.clone(),
        name: "linux-vorpal".to_string(),
        sources: vec![],
        steps: vec![
            steps::bwrap(
                vec![],
                artifacts,
                vec![ArtifactEnvironment {
                    key: "PATH".to_string(),
                    value: "/usr/bin:/usr/sbin".to_string(),
                }],
                Some(get_artifact_envkey(linux_debian)),
                script::generate(
                    &bash,
                    &binutils,
                    &coreutils,
                    &diffutils,
                    &file,
                    &findutils,
                    &gawk,
                    &gcc,
                    &glibc,
                    &grep,
                    &gzip,
                    &linux_headers,
                    &m4,
                    &make,
                    &ncurses,
                    &patch,
                    &sed,
                    &tar,
                    &xz,
                ),
            ),
            steps::bwrap(
                vec![
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
                    // set group id
                    "--gid".to_string(),
                    "0".to_string(),
                    // set user id
                    "--uid".to_string(),
                    "0".to_string(),
                ],
                vec![
                    bison.clone(),
                    curl.clone(),
                    curl_cacert.clone(),
                    gettext.clone(),
                    libidn2.clone(),
                    libpsl.clone(),
                    libunistring.clone(),
                    openssl.clone(),
                    perl.clone(),
                    python.clone(),
                    texinfo.clone(),
                    unzip.clone(),
                    util_linux.clone(),
                    zlib.clone(),
                ],
                vec![ArtifactEnvironment {
                    key: "PATH".to_string(),
                    value: "/usr/bin:/usr/sbin".to_string(),
                }],
                None,
                script::generate_post(
                    &bison,
                    &curl,
                    &curl_cacert,
                    &gettext,
                    &libidn2,
                    &libpsl,
                    &libunistring,
                    &openssl,
                    &perl,
                    &python,
                    &texinfo,
                    &unzip,
                    &util_linux,
                    &zlib,
                ),
            ),
        ],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    })
}
