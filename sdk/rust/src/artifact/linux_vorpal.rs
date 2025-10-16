use crate::{
    api::artifact::ArtifactSystem::{Aarch64Linux, X8664Linux},
    artifact::{
        linux_debian,
        linux_vorpal::script::{setup, stage_01, stage_02, stage_03, stage_04, stage_05},
        step, Artifact,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

mod script;
mod source;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let bash_version = "5.2.32";
    let bash = source::gnu("bash", bash_version);

    let binutils_version = "2.43.1";
    let binutils = source::gnu("binutils", binutils_version);

    let bison_version = "3.8.2";
    let bison = source::gnu("bison", bison_version);

    let coreutils_version = "9.5";
    let coreutils = source::gnu("coreutils", coreutils_version);

    let curl_version = "8.11.0";
    let curl = source::curl(curl_version);

    let curl_cacert = source::curl_cacert();

    let diffutils_version = "3.10";
    let diffutils = source::gnu_xz("diffutils", diffutils_version);

    let file_version = "5.45";
    let file = source::file(file_version);

    let findutils_version = "4.10.0";
    let findutils = source::gnu_xz("findutils", findutils_version);

    let gawk_version = "5.3.0";
    let gawk = source::gnu("gawk", gawk_version);

    let gcc_version = "14.2.0";
    let gcc = source::gnu_gcc(gcc_version);

    let gettext_version = "0.22.5";
    let gettext = source::gnu("gettext", gettext_version);

    let glibc_version = "2.40";
    let glibc = source::gnu("glibc", glibc_version);

    let glibc_patch = source::gnu_glibc_patch(glibc_version);

    let gmp_version = "6.3.0";
    let gmp = source::gnu("gmp", gmp_version);

    let grep_version = "3.11";
    let grep = source::gnu("grep", grep_version);

    let gzip_version = "1.13";
    let gzip = source::gnu("gzip", gzip_version);

    let libidn2_version = "2.3.7";
    let libidn2 = source::libidn2(libidn2_version);

    let libpsl_version = "0.21.5";
    let libpsl = source::libpsl(libpsl_version);

    let libunistring_version = "1.2";
    let libunistring = source::gnu("libunistring", libunistring_version);

    let linux_version = "6.10.5";
    let linux = source::linux(linux_version);

    let m4_version = "1.4.19";
    let m4 = source::gnu("m4", m4_version);

    let make_version = "4.4.1";
    let make = source::gnu("make", make_version);

    let mpc_version = "1.3.1";
    let mpc = source::gnu("mpc", mpc_version);

    let mpfr_version = "4.2.1";
    let mpfr = source::gnu_xz("mpfr", mpfr_version);

    let ncurses_version = "6.5";
    let ncurses = source::ncurses(ncurses_version);

    let openssl_version = "3.3.1";
    let openssl = source::openssl(openssl_version);

    let patch_version = "2.7.6";
    let patch = source::gnu("patch", "2.7.6");

    let perl_version = "5.40.0";
    let perl = source::perl(perl_version);

    let python_version = "3.12.5";
    let python = source::python(python_version);

    let sed_version = "4.9";
    let sed = source::gnu("sed", sed_version);

    let tar_version = "1.35";
    let tar = source::gnu("tar", tar_version);

    let texinfo_version = "7.1.1";
    let texinfo = source::gnu("texinfo", texinfo_version);

    let unzip_version = "6.0";
    let unzip = source::unzip(unzip_version);

    let unzip_patch_fixes = source::unzip_patch_fixes("6.0");

    let unzip_patch_gcc14 = source::unzip_patch_gcc14("6.0");

    let util_linux_version = "2.40.2";
    let util_linux = source::util_linux(util_linux_version);

    let xz_version = "5.6.2";
    let xz = source::xz(xz_version);

    let zlib_version = "1.3.1";
    let zlib = source::zlib(zlib_version);

    let step_environments = vec!["PATH=/usr/bin:/usr/sbin".to_string()];
    let step_rootfs = linux_debian::build(context).await?;

    let step_setup_script = setup::script(
        binutils_version,
        gawk_version,
        gcc_version,
        glibc_version,
        gmp_version,
        mpc_version,
        mpfr_version,
        ncurses_version,
    );

    let step_stage_01_script =
        stage_01::script(binutils_version, gcc_version, glibc_version, linux_version);

    let step_stage_02_script = stage_02::script(
        bash_version,
        binutils_version,
        coreutils_version,
        diffutils_version,
        file_version,
        findutils_version,
        gawk_version,
        gcc_version,
        grep_version,
        gzip_version,
        m4_version,
        make_version,
        ncurses_version,
        patch_version,
        sed_version,
        tar_version,
        xz_version,
    );

    let bwrap_arguments = vec![
        // mount bin
        "--bind",
        "$VORPAL_OUTPUT/bin",
        "/bin",
        // mount etc
        "--bind",
        "$VORPAL_OUTPUT/etc",
        "/etc",
        // mount lib
        "--bind",
        "$VORPAL_OUTPUT/lib",
        "/lib",
        // mount lib64 (if exists)
        "--bind-try",
        "$VORPAL_OUTPUT/lib64",
        "/lib64",
        // mount sbin
        "--bind",
        "$VORPAL_OUTPUT/sbin",
        "/sbin",
        // mount usr
        "--bind",
        "$VORPAL_OUTPUT/usr",
        "/usr",
        // mount current directory
        "--bind",
        "$VORPAL_WORKSPACE",
        "$VORPAL_WORKSPACE",
        // change directory
        "--chdir",
        "$VORPAL_WORKSPACE",
        // set group id
        "--gid",
        "0",
        // set user id
        "--uid",
        "0",
    ];

    let step_stage_03_script = stage_03::script(
        bison_version,
        gettext_version,
        perl_version,
        python_version,
        texinfo_version,
        util_linux_version,
    );

    let step_stage_04_script = stage_04::script(
        binutils_version,
        gcc_version,
        glibc_version,
        openssl_version,
        zlib_version,
    );

    let step_stage_05_script = stage_05::script(
        curl_version,
        libidn2_version,
        libpsl_version,
        libunistring_version,
        unzip_version,
    );

    let systems = vec![Aarch64Linux, X8664Linux];

    // TODO: impove readability with list in list

    let steps = vec![
        step::bwrap(
            vec![],
            vec![],
            step_environments.clone(),
            Some(step_rootfs.clone()),
            vec![],
            step_setup_script,
        )
        .await?,
        step::bwrap(
            vec![],
            vec![],
            step_environments.clone(),
            Some(step_rootfs.clone()),
            vec![],
            step_stage_01_script,
        )
        .await?,
        step::bwrap(
            vec![],
            vec![],
            step_environments.clone(),
            Some(step_rootfs.clone()),
            vec![],
            step_stage_02_script,
        )
        .await?,
        step::bwrap(
            [
                bwrap_arguments.clone(),
                vec![
                    // mount tools
                    "--bind",
                    "$VORPAL_OUTPUT/tools",
                    "/tools",
                ],
            ]
            .concat(),
            vec![],
            step_environments.clone(),
            None,
            vec![],
            step_stage_03_script,
        )
        .await?,
        step::bwrap(
            vec![],
            vec![],
            step_environments.clone(),
            Some(step_rootfs.clone()),
            vec![],
            formatdoc! {"
                rm -rf $VORPAL_OUTPUT/tools",
            },
        )
        .await?,
        step::bwrap(
            bwrap_arguments.clone(),
            vec![],
            step_environments.clone(),
            None,
            vec![],
            step_stage_04_script,
        )
        .await?,
        step::bwrap(
            bwrap_arguments.clone(),
            vec![],
            step_environments.clone(),
            None,
            vec![],
            step_stage_05_script,
        )
        .await?,
    ];

    let name = "linux-vorpal";

    Artifact::new(name, steps, systems)
        .with_aliases(vec![format!("{name}:latest")])
        .with_sources(vec![
            bash,
            binutils,
            bison,
            coreutils,
            curl,
            curl_cacert,
            diffutils,
            file,
            findutils,
            gawk,
            gcc,
            gettext,
            glibc,
            glibc_patch,
            gmp,
            grep,
            gzip,
            libidn2,
            libpsl,
            libunistring,
            linux,
            m4,
            make,
            mpc,
            mpfr,
            ncurses,
            openssl,
            patch,
            perl,
            python,
            sed,
            tar,
            texinfo,
            unzip,
            unzip_patch_fixes,
            unzip_patch_gcc14,
            util_linux,
            xz,
            zlib,
        ])
        .build(context)
        .await
}
