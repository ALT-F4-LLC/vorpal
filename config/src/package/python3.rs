use crate::{
    cross_platform::get_cpu_count,
    sandbox::scripts::{add_scripts, PackageRpath},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

#[allow(clippy::too_many_arguments)]
pub fn package(
    context: &mut ContextConfig,
    target: PackageSystem,
    bash: &PackageOutput,
    binutils: &PackageOutput,
    bison: &PackageOutput,
    coreutils: &PackageOutput,
    diffutils: &PackageOutput,
    file: &PackageOutput,
    findutils: &PackageOutput,
    gawk: &PackageOutput,
    gcc: &PackageOutput,
    gettext: &PackageOutput,
    glibc: &PackageOutput,
    grep: &PackageOutput,
    gzip: &PackageOutput,
    libstdcpp: &PackageOutput,
    linux_headers: &PackageOutput,
    m4: &PackageOutput,
    make: &PackageOutput,
    ncurses: &PackageOutput,
    patch: &PackageOutput,
    patchelf: &PackageOutput,
    perl: &PackageOutput,
    sed: &PackageOutput,
    tar: &PackageOutput,
    xz: &PackageOutput,
) -> Result<PackageOutput> {
    let environment = vec![PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let name = "python-stage-01";

    let sandbox = PackageSandbox { paths: vec![] };

    let script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail

        mkdir -pv /bin
        mkdir -pv /lib
        mkdir -pv /lib64
        mkdir -pv /usr/bin

        ln -s ${bash}/bin/bash /bin/bash
        ln -s ${bash}/bin/bash /bin/sh
        ln -s ${gcc}/bin/cpp /lib/cpp
        ln -s ${glibc}/lib/ld-linux-aarch64.so.1 /lib/ld-linux-aarch64.so.1
        ln -s ${glibc}/lib/ld-linux-aarch64.so.1 /lib64/ld-linux-aarch64.so.1

        export I18NPATH=${glibc}/share/i18n
        mkdir -pv \"$output/C.UTF-8\"
        localedef -i POSIX -f UTF-8 \"$output/C.UTF-8\" || true

        export LOCPATH=\"$output\"
        export LC_ALL=\"C.UTF-8\"

        export C_INCLUDE_PATH=\"${glibc}/include:${linux_headers}/usr/include\"
        export CPPFLAGS=\"-I${glibc}/include -I${linux_headers}/usr/include\"

        cd \"${{PWD}}/{source}\"

        ./configure \
            --enable-shared \
            --prefix=\"$output\" \
            --without-ensurepip

        make -j$({cores})
        make install",
        bash = bash.name.to_lowercase().replace("-", "_"),
        cores = get_cpu_count(target)?,
        gcc = gcc.name.to_lowercase().replace("-", "_"),
        glibc = glibc.name.to_lowercase().replace("-", "_"),
        linux_headers = linux_headers.name.to_lowercase().replace("-", "_"),
        source = name,
    };

    let source = PackageSource {
        excludes: vec![],
        hash: Some("520126b87d4deb735ee9e269f1a21fc583a90742968bf2a826f6b6114b5710ed".to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: true,
        uri: "https://www.python.org/ftp/python/3.12.7/Python-3.12.7.tar.xz".to_string(),
    };

    let package = Package {
        environment,
        name: name.to_string(),
        packages: vec![
            bash.clone(),
            binutils.clone(),
            bison.clone(),
            coreutils.clone(),
            diffutils.clone(),
            file.clone(),
            findutils.clone(),
            gawk.clone(),
            gcc.clone(),
            gettext.clone(),
            glibc.clone(),
            grep.clone(),
            gzip.clone(),
            libstdcpp.clone(),
            linux_headers.clone(),
            m4.clone(),
            make.clone(),
            ncurses.clone(),
            patch.clone(),
            patchelf.clone(),
            perl.clone(),
            sed.clone(),
            tar.clone(),
            xz.clone(),
        ],
        sandbox: Some(sandbox),
        script,
        source: vec![source],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    let gcc_env_key = gcc.name.to_lowercase().replace("-", "_");
    let glibc_env_key = glibc.name.to_lowercase().replace("-", "_");

    let package_rpaths = vec![
        PackageRpath {
            rpath: format!("$output/lib:${}/lib:${}/lib64", glibc_env_key, gcc_env_key),
            shrink: true,
            target: "$output/bin".to_string(),
        },
        PackageRpath {
            rpath: format!("${}/lib:${}/lib64", glibc_env_key, gcc_env_key),
            shrink: true,
            target: "$output/lib".to_string(),
        },
    ];

    let package = add_scripts(package, target, Some(glibc), package_rpaths)?;

    let package_output = context.add_package(package)?;

    Ok(package_output)
}
