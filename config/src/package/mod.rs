use crate::{
    cross_platform::get_sed_cmd,
    sandbox::{environments::add_environments, packages::add_packages, scripts::add_scripts},
    ContextConfig,
};
use anyhow::Result;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub mod bash_stage_01;
pub mod binutils_stage_01;
pub mod binutils_stage_02;
pub mod bison_stage_01;
pub mod cargo;
pub mod coreutils_stage_01;
pub mod diffutils_stage_01;
pub mod file_stage_01;
pub mod findutils_stage_01;
pub mod gawk_stage_01;
pub mod gcc_stage_01;
pub mod gcc_stage_02;
pub mod gettext_stage_01;
pub mod glibc_stage_01;
pub mod grep_stage_01;
pub mod gzip_stage_01;
pub mod language;
pub mod libstdcpp_stage_01;
pub mod linux_headers;
pub mod m4_stage_01;
pub mod make_stage_01;
pub mod ncurses_stage_01;
pub mod patch_stage_01;
pub mod patchelf_stage_01;
pub mod perl_stage_01;
pub mod protoc;
pub mod python_stage_01;
pub mod rust_std;
pub mod rustc;
pub mod sed_stage_01;
pub mod tar_stage_01;
pub mod texinfo_stage_01;
pub mod util_linux_stage_01;
pub mod xz_stage_01;
// pub mod zlib_stage_01;

pub fn build_package(
    context: &mut ContextConfig,
    package: Package,
    target: PackageSystem,
) -> Result<PackageOutput> {
    let mut package = package.clone();

    let mut bash = None;
    let mut binutils = None;
    let mut bison = None;
    let mut coreutils = None;
    let mut diffutils = None;
    let mut file = None;
    let mut findutils = None;
    let mut gawk = None;
    let mut gcc = None;
    let mut gettext = None;
    let mut glibc = None;
    let mut grep = None;
    let mut gzip = None;
    let mut libstdcpp = None;
    let mut linux_headers = None;
    let mut m4 = None;
    let mut make = None;
    let mut ncurses = None;
    let mut patch = None;
    let mut patchelf = None;
    let mut perl = None;
    let mut python = None;
    let mut sed = None;
    let mut tar = None;
    let mut texinfo = None;
    let mut util_linux = None;
    let mut xz = None;
    // let mut zlib = None;

    if target == Aarch64Macos || target == X8664Macos {
        let bash_package =
            bash_stage_01::package(context, target, None, None, None, None, None, None, None)?;

        let coreutils_package = coreutils_stage_01::package(
            context,
            target,
            &bash_package,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )?;

        bash = Some(bash_package);
        coreutils = Some(coreutils_package);
    }

    if target == Aarch64Linux || target == X8664Linux {
        let binutils_package = binutils_stage_01::package(context, target)?;

        let gcc_package = gcc_stage_01::package(context, target, &binutils_package)?;

        let linux_headers_package =
            linux_headers::package(context, target, &binutils_package, &gcc_package)?;

        let glibc_package = glibc_stage_01::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &linux_headers_package,
        )?;

        let libstdcpp_package = libstdcpp_stage_01::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &glibc_package,
            &linux_headers_package,
        )?;

        let m4_package = m4_stage_01::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
        )?;

        let ncurses_package = ncurses_stage_01::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
        )?;

        let bash_package = bash_stage_01::package(
            context,
            target,
            Some(&binutils_package),
            Some(&gcc_package),
            Some(&glibc_package),
            Some(&libstdcpp_package),
            Some(&linux_headers_package),
            Some(&m4_package),
            Some(&ncurses_package),
        )?;

        let coreutils_package = coreutils_stage_01::package(
            context,
            target,
            &bash_package,
            Some(&binutils_package),
            Some(&gcc_package),
            Some(&glibc_package),
            Some(&libstdcpp_package),
            Some(&linux_headers_package),
            Some(&m4_package),
            Some(&ncurses_package),
        )?;

        let diffutils_package = diffutils_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
        )?;

        let file_package = file_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
        )?;

        let findutils_package = findutils_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &file_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
        )?;

        let gawk_package = gawk_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &file_package,
            &findutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
        )?;

        let grep_package = grep_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
        )?;

        let gzip_package = gzip_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
        )?;

        let make_package = make_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
        )?;

        let patch_package = patch_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
        )?;

        let sed_package = sed_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
        )?;

        let tar_package = tar_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &sed_package,
        )?;

        let xz_package = xz_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &sed_package,
            &tar_package,
        )?;

        let binutils_package = binutils_stage_02::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let gcc_package = gcc_stage_02::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let patchelf_package = patchelf_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let gettext_package = gettext_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &patchelf_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let bison_package = bison_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &gettext_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &patchelf_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let perl_package = perl_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &bison_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &gettext_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &patchelf_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let python_package = python_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &bison_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &gettext_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &patchelf_package,
            &perl_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let texinfo_package = texinfo_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &bison_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &gettext_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &perl_package,
            &python_package,
            &sed_package,
            &tar_package,
            &xz_package,
        )?;

        let util_linux_package = util_linux_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &bison_package,
            &coreutils_package,
            &diffutils_package,
            &file_package,
            &findutils_package,
            &gawk_package,
            &gcc_package,
            &gettext_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &make_package,
            &ncurses_package,
            &patch_package,
            &perl_package,
            &python_package,
            &sed_package,
            &tar_package,
            &texinfo_package,
            &xz_package,
        )?;

        // let zlib_package = zlib_stage_01::package(context, target)?;

        bash = Some(bash_package);
        binutils = Some(binutils_package);
        bison = Some(bison_package);
        coreutils = Some(coreutils_package.clone());
        diffutils = Some(diffutils_package);
        file = Some(file_package);
        findutils = Some(findutils_package);
        gawk = Some(gawk_package);
        gcc = Some(gcc_package);
        gettext = Some(gettext_package);
        glibc = Some(glibc_package);
        grep = Some(grep_package);
        gzip = Some(gzip_package);
        libstdcpp = Some(libstdcpp_package);
        linux_headers = Some(linux_headers_package);
        m4 = Some(m4_package);
        make = Some(make_package);
        ncurses = Some(ncurses_package);
        patch = Some(patch_package);
        patchelf = Some(patchelf_package);
        perl = Some(perl_package);
        python = Some(python_package);
        sed = Some(sed_package);
        tar = Some(tar_package);
        texinfo = Some(texinfo_package);
        util_linux = Some(util_linux_package);
        xz = Some(xz_package);
        // zlib = Some(zlib_package);
    }

    package = add_environments(
        package,
        bash.as_ref(),
        binutils.as_ref(),
        gcc.as_ref(),
        glibc.as_ref(),
        libstdcpp.as_ref(),
        linux_headers.as_ref(),
        ncurses.as_ref(),
        // zlib.as_ref(),
    );

    package = add_packages(
        package,
        target,
        bash.as_ref(),
        binutils.as_ref(),
        bison.as_ref(),
        coreutils.as_ref(),
        diffutils.as_ref(),
        file.as_ref(),
        findutils.as_ref(),
        gawk.as_ref(),
        gcc.as_ref(),
        gettext.as_ref(),
        glibc.as_ref(),
        grep.as_ref(),
        gzip.as_ref(),
        libstdcpp.as_ref(),
        linux_headers.as_ref(),
        m4.as_ref(),
        make.as_ref(),
        ncurses.as_ref(),
        patch.as_ref(),
        patchelf.as_ref(),
        perl.as_ref(),
        python.as_ref(),
        sed.as_ref(),
        tar.as_ref(),
        texinfo.as_ref(),
        util_linux.as_ref(),
        xz.as_ref(),
        // zlib.as_ref(),
    )?;

    package = add_scripts(package, target, glibc.as_ref(), vec![])?;

    package = Package {
        environment: package.environment,
        name: package.name,
        packages: package.packages,
        sandbox: package.sandbox,
        script: package.script,
        source: package.source,
        systems: package.systems,
    };

    context.add_package(package.clone())
}
