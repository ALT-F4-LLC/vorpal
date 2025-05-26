use crate::{
    api::artifact::ArtifactSystem::{Aarch64Linux, X8664Linux},
    artifact::{
        linux_debian,
        linux_vorpal::script::{setup, stage_01, stage_02, stage_03, stage_04, stage_05},
        step, ArtifactBuilder,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

mod script;
mod source;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let bash_version = "5.2.32";
    let bash = source::gnu(
        "bash",
        bash_version,
        "19a8087c947a587b491508a6675a5349e23992d5dfca40a0bd0735bbd81e0438",
    );

    let binutils_version = "2.43.1";
    let binutils = source::gnu(
        "binutils",
        binutils_version,
        "c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528",
    );

    let bison_version = "3.8.2";
    let bison = source::gnu(
        "bison",
        bison_version,
        "cb18c2c8562fc01bf3ae17ffe9cf8274e3dd49d39f89397c1a8bac7ee14ce85f",
    );

    let coreutils_version = "9.5";
    let coreutils = source::gnu(
        "coreutils",
        coreutils_version,
        "af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4",
    );

    let curl_version = "8.11.0";
    let curl = source::curl(
        curl_version,
        "97dde4e45e89291bf5405b0363b16049333366f286a1989537441c261e9299fe",
    );

    let curl_cacert =
        source::curl_cacert("7f5218a225d0451ff8bd0b57d8a4c63b1c4ba52a6a8887cd2700cc3aef9431c1");

    let diffutils_version = "3.10";
    let diffutils = source::gnu_xz(
        "diffutils",
        diffutils_version,
        "5045e29e7fa0ffe017f63da7741c800cbc0f89e04aebd78efcd661d6e5673326",
    );

    let file_version = "5.45";
    let file = source::file(
        file_version,
        "c118ab56efa05798022a5a488827594a82d844f65159e95b918d5501adf1e58f",
    );

    let findutils_version = "4.10.0";
    let findutils = source::gnu_xz(
        "findutils",
        findutils_version,
        "242f804d87a5036bb0fab99966227dc61e853e5a67e1b10c3cc45681c792657e",
    );

    let gawk_version = "5.3.0";
    let gawk = source::gnu(
        "gawk",
        gawk_version,
        "a21e5899707ddc030a0fcc0a35c95a9602dca1a681fa52a1790a974509b40133",
    );

    let gcc_version = "14.2.0";
    let gcc = source::gnu_gcc(
        gcc_version,
        "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583",
    );

    let gettext_version = "0.22.5";
    let gettext = source::gnu(
        "gettext",
        gettext_version,
        "6e3ef842d1006a6af7778a8549a8e8048fc3b923e5cf48eaa5b82b5d142220ae",
    );

    let glibc_version = "2.40";
    let glibc = source::gnu(
        "glibc",
        glibc_version,
        "da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b",
    );

    let glibc_patch = source::gnu_glibc_patch(
        glibc_version,
        "69cf0653ad0a6a178366d291f30629d4e1cb633178aa4b8efbea0c851fb944ca",
    );

    let gmp_version = "6.3.0";
    let gmp = source::gnu(
        "gmp",
        gmp_version,
        "191226cef6e9ce60e291e178db47682aadea28cb3e92f35f006ba317f3e10195",
    );

    let grep_version = "3.11";
    let grep = source::gnu(
        "grep",
        grep_version,
        "1625eae01f6e4dbc41b58545aa2326c74791b2010434f8241d41903a4ea5ff70",
    );

    let gzip_version = "1.13";
    let gzip = source::gnu(
        "gzip",
        gzip_version,
        "25e51d46402bab819045d452ded6c4558ef980f5249c470d9499e9eae34b59b1",
    );

    let libidn2_version = "2.3.7";
    let libidn2 = source::libidn2(
        libidn2_version,
        "cb09b889bc9e51a2f5ec9d04dbbf03582926a129340828271955d15a57da6a3c",
    );

    let libpsl_version = "0.21.5";
    let libpsl = source::libpsl(
        libpsl_version,
        "65ecfe61646c50119a018a2003149833c11387efd92462f974f1ff9f907c1d78",
    );

    let libunistring_version = "1.2";
    let libunistring = source::gnu(
        "libunistring",
        libunistring_version,
        "c621c94a94108095cfe08cc61f484d4b4cb97824c64a4e2bb1830d8984b542f3",
    );

    let linux_version = "6.10.5";
    let linux = source::linux(
        linux_version,
        "b1548c4f5bf63c5f44c1a8c3044842a49ef445deb1b3da55b8116200a25793be",
    );

    let m4_version = "1.4.19";
    let m4 = source::gnu(
        "m4",
        m4_version,
        "fd793cdfc421fac76f4af23c7d960cbe4a29cbb18f5badf37b85e16a894b3b6d",
    );

    let make_version = "4.4.1";
    let make = source::gnu(
        "make",
        make_version,
        "8dfe7b0e51b3e190cd75e046880855ac1be76cf36961e5cfcc82bfa91b2c3ba8",
    );

    let mpc_version = "1.3.1";
    let mpc = source::gnu(
        "mpc",
        mpc_version,
        "c179fbcd6e48931a16c0af37d0c4a5e1688dd07d71e2b4a532c68cd5edbb5b72",
    );

    let mpfr_version = "4.2.1";
    let mpfr = source::gnu_xz(
        "mpfr",
        mpfr_version,
        "8e3814a6595d335c49b39f5777c6783ba1cd2e57fb3a1696f009b4e5f45f97d4",
    );

    let ncurses_version = "6.5";
    let ncurses = source::ncurses(
        ncurses_version,
        "aab234a3b7a22e2632151fbe550cb36e371d3ee5318a633ee43af057f9f112fb",
    );

    let openssl_version = "3.3.1";
    let openssl = source::openssl(
        openssl_version,
        "a53e2254e36124452582477935a680f07f9884fe1d6e9ec03c28ac71b750d84a",
    );

    let patch_version = "2.7.6";
    let patch = source::gnu(
        "patch",
        "2.7.6",
        "af8c281a05a6802075799c0c179e5fb3a218be6a21b726d8b672cd0f4c37eae9",
    );

    let perl_version = "5.40.0";
    let perl = source::perl(
        perl_version,
        "59b6437a3da1d9de0126135b31f1f16aee9c3b7a0f61f6364b2da3e8bb5f771f",
    );

    let python_version = "3.12.5";
    let python = source::python(
        python_version,
        "8359773924d33702ecd6f9fab01973e53d929d46d7cdc4b0df31eb1282c68b67",
    );

    let sed_version = "4.9";
    let sed = source::gnu(
        "sed",
        sed_version,
        "434ff552af89340088e0d8cb206c251761297909bbee401176bc8f655e8e7cf2",
    );

    let tar_version = "1.35";
    let tar = source::gnu(
        "tar",
        tar_version,
        "f9bb5f39ed45b1c6a324470515d2ef73e74422c5f345503106d861576d3f02f3",
    );

    let texinfo_version = "7.1.1";
    let texinfo = source::gnu(
        "texinfo",
        texinfo_version,
        "6e34604552af91db0b4ccf0bcceba63dd3073da2a492ebcf33c6e188a64d2b63",
    );

    let unzip_version = "6.0";
    let unzip = source::unzip(
        unzip_version,
        "4585067be297ae977da3f81587fcf0a141a8d6ceb6137d199255683ed189c3ed",
    );

    let unzip_patch_fixes = source::unzip_patch_fixes(
        "6.0",
        "11350935be5bbb743f1a97ec069b78fc2904f92b24abbc7fb3d7f0ff8bb889ea",
    );

    let unzip_patch_gcc14 = source::unzip_patch_gcc14(
        "6.0",
        "d6ac941672086ea4c8d5047d550b40047825a685cc7c48626d2f0939e1a0c797",
    );

    let util_linux_version = "2.40.2";
    let util_linux = source::util_linux(
        util_linux_version,
        "7db19a1819ac5c743b52887a4571e42325b2bfded63d93b6a1797ae2b1f8019a",
    );

    let xz_version = "5.6.2";
    let xz = source::xz(
        xz_version,
        "7a02b1278ed9a59b332657d613c5549b39afe34e315197f4da95c5322524ec26",
    );

    let zlib_version = "1.3.1";
    let zlib = source::zlib(
        zlib_version,
        "3f7995d5f103719283f509c23624287ce95c349439e881ed935a3c2c807bb683",
    );

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

    ArtifactBuilder::new(name, steps, systems)
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
