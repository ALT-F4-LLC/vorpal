use crate::config::artifact::{get_artifact_envkey, steps, ConfigContext};
use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

mod script;
mod source;

pub fn artifact(context: &mut ConfigContext, linux_debian: &ArtifactId) -> Result<ArtifactId> {
    let bash_source = source::bash(context, linux_debian)?;
    let binutils_source = source::binutils(context, linux_debian)?;
    let bison_source = source::bison(context, linux_debian)?;
    let coreutils_source = source::coreutils(context, linux_debian)?;
    let curl_source = source::curl(context, linux_debian)?;
    let curl_cacert_source = source::curl_cacert(context, linux_debian)?;
    let diffutils_source = source::diffutils(context, linux_debian)?;
    let file_source = source::file(context, linux_debian)?;
    let findutils_source = source::findutils(context, linux_debian)?;
    let gawk_source = source::gawk(context, linux_debian)?;
    let gcc_source = source::gcc(context, linux_debian)?;
    let gettext_source = source::gettext(context, linux_debian)?;
    let glibc_patch_source = source::glibc_patch(context, linux_debian)?;
    let glibc_source = source::glibc(context, &glibc_patch_source, linux_debian)?;
    let grep_source = source::grep(context, linux_debian)?;
    let gzip_source = source::gzip(context, linux_debian)?;
    let libidn2_source = source::libidn2(context, linux_debian)?;
    let libpsl_source = source::libpsl(context, linux_debian)?;
    let libunistring_source = source::libunistring(context, linux_debian)?;
    let linux_source = source::linux(context, linux_debian)?;
    let m4_source = source::m4(context, linux_debian)?;
    let make_source = source::make(context, linux_debian)?;
    let ncurses_source = source::ncurses(context, linux_debian)?;
    let openssl_source = source::openssl(context, linux_debian)?;
    let patch_source = source::patch(context, linux_debian)?;
    let perl_source = source::perl(context, linux_debian)?;
    let python_source = source::python(context, linux_debian)?;
    let sed_source = source::sed(context, linux_debian)?;
    let tar_source = source::tar(context, linux_debian)?;
    let texinfo_source = source::texinfo(context, linux_debian)?;
    let unzip_patch_fixes_source = source::unzip_patch_fixes(context, linux_debian)?;
    let unzip_patch_gcc14_source = source::unzip_patch_gcc14(context, linux_debian)?;
    let unzip_source = source::unzip(
        context,
        linux_debian,
        &unzip_patch_fixes_source,
        &unzip_patch_gcc14_source,
    )?;
    let util_linux_source = source::util_linux(context, linux_debian)?;
    let xz_source = source::xz(context, linux_debian)?;
    let zlib_source = source::zlib(context, linux_debian)?;

    // TODO: implement "expect_hash" for artifactIds

    let artifacts = vec![
        bash_source.clone(),
        binutils_source.clone(),
        bison_source.clone(),
        coreutils_source.clone(),
        curl_source.clone(),
        curl_cacert_source.clone(),
        diffutils_source.clone(),
        file_source.clone(),
        findutils_source.clone(),
        gawk_source.clone(),
        gcc_source.clone(),
        gettext_source.clone(),
        glibc_source.clone(),
        grep_source.clone(),
        gzip_source.clone(),
        libidn2_source.clone(),
        libpsl_source.clone(),
        libunistring_source.clone(),
        linux_debian.clone(),
        linux_source.clone(),
        m4_source.clone(),
        make_source.clone(),
        ncurses_source.clone(),
        openssl_source.clone(),
        patch_source.clone(),
        perl_source.clone(),
        python_source.clone(),
        sed_source.clone(),
        tar_source.clone(),
        texinfo_source.clone(),
        unzip_source.clone(),
        util_linux_source.clone(),
        xz_source.clone(),
        zlib_source.clone(),
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
                    &bash_source,
                    &binutils_source,
                    &coreutils_source,
                    &diffutils_source,
                    &file_source,
                    &findutils_source,
                    &gawk_source,
                    &gcc_source,
                    &glibc_source,
                    &grep_source,
                    &gzip_source,
                    &linux_source,
                    &m4_source,
                    &make_source,
                    &ncurses_source,
                    &patch_source,
                    &sed_source,
                    &tar_source,
                    &xz_source,
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
                    bison_source.clone(),
                    curl_source.clone(),
                    curl_cacert_source.clone(),
                    gettext_source.clone(),
                    libidn2_source.clone(),
                    libpsl_source.clone(),
                    libunistring_source.clone(),
                    openssl_source.clone(),
                    perl_source.clone(),
                    python_source.clone(),
                    texinfo_source.clone(),
                    unzip_source.clone(),
                    util_linux_source.clone(),
                    zlib_source.clone(),
                ],
                vec![ArtifactEnvironment {
                    key: "PATH".to_string(),
                    value: "/usr/bin:/usr/sbin".to_string(),
                }],
                None,
                script::generate_post(
                    &bison_source,
                    &curl_source,
                    &curl_cacert_source,
                    &gettext_source,
                    &libidn2_source,
                    &libpsl_source,
                    &libunistring_source,
                    &openssl_source,
                    &perl_source,
                    &python_source,
                    &texinfo_source,
                    &unzip_source,
                    &util_linux_source,
                    &zlib_source,
                ),
            ),
        ],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    })
}
