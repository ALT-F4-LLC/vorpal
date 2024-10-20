use crate::{cross_platform::get_sed_cmd, ContextConfig};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub mod bash_stage_01;
pub mod binutils_stage_01;
pub mod cargo;
pub mod coreutils_stage_01;
pub mod diffutils_stage_01;
pub mod file_stage_01;
pub mod findutils_stage_01;
pub mod gawk_stage_01;
pub mod gcc_stage_01;
pub mod glibc_stage_01;
pub mod grep_stage_01;
pub mod gzip_stage_01;
pub mod language;
pub mod libstdcpp_stage_01;
pub mod linux_headers;
pub mod m4_stage_01;
pub mod ncurses_stage_01;
pub mod patchelf_stage_01;
pub mod zlib_stage_01;
// pub mod zstd_stage_01;
pub mod protoc;
pub mod rust_std;
pub mod rustc;

#[allow(clippy::too_many_arguments)]
fn add_default_environment(
    package: Package,
    bash: Option<&PackageOutput>,
    binutils: Option<&PackageOutput>,
    gcc: Option<&PackageOutput>,
    glibc: Option<&PackageOutput>,
    libstdcpp: Option<&PackageOutput>,
    linux_headers: Option<&PackageOutput>,
    ncurses: Option<&PackageOutput>,
    zlib: Option<&PackageOutput>,
) -> Package {
    let mut environment = vec![];

    let lc_all = PackageEnvironment {
        key: "LC_ALL".to_string(),
        value: "C".to_string(),
    };

    environment.push(lc_all);

    let c_include_path_key = "C_INCLUDE_PATH".to_string();
    let cppflags_key = "CPPFLAGS".to_string();
    let ld_library_path_key = "LD_LIBRARY_PATH".to_string();
    let ldflags_key = "LDFLAGS".to_string();
    let library_path_key = "LIBRARY_PATH".to_string();
    let pkg_config_path_key = "PKG_CONFIG_PATH".to_string();

    let mut c_include_paths = vec![];
    let mut cppflags_args = vec![];
    let mut ld_library_paths = vec![];
    let mut ldflags_args = vec![];
    let mut library_paths = vec![];
    let mut pkg_config_paths = vec![];

    if let Some(bash) = bash {
        let env_key = format!("${}", bash.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path);
    }

    if let Some(binutils) = binutils {
        let env_key = format!("${}", binutils.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path.clone());
    }

    if let Some(gcc) = gcc {
        let env_key = format!("${}", gcc.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);
        let lib64_path = format!("{}/lib64", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ld_library_paths.push(lib64_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        ldflags_args.push(format!("-L{}", lib64_path));
        library_paths.push(lib_path.clone());
        library_paths.push(lib64_path.clone());

        let cc_key = "CC".to_string();
        let gcc_key = "GCC".to_string();
        let gcc_path = format!("{}/bin/gcc", env_key);

        let cc = PackageEnvironment {
            key: cc_key.clone(),
            value: gcc_path.clone(),
        };

        let gcc = PackageEnvironment {
            key: gcc_key.clone(),
            value: gcc_path.clone(),
        };

        environment.push(cc);
        environment.push(gcc);
    }

    if let Some(glibc) = glibc {
        let env_key = format!("${}", glibc.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path.clone());
    }

    if let Some(libstdcpp) = libstdcpp {
        let env_key = format!("${}", libstdcpp.name.to_lowercase().replace("-", "_"));
        let lib_path = format!("{}/lib", env_key);
        let lib64_path = format!("{}/lib64", env_key);

        ld_library_paths.push(lib_path.clone());
        ld_library_paths.push(lib64_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        ldflags_args.push(format!("-L{}", lib64_path));
        library_paths.push(lib_path.clone());
        library_paths.push(lib64_path.clone());
    }

    if let Some(linux_headers) = linux_headers {
        let env_key = format!("${}", linux_headers.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
    }

    if let Some(ncurses) = ncurses {
        let env_key = format!("${}", ncurses.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include/ncursesw", env_key);
        let lib_path = format!("{}/lib", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path.clone());
    }

    if let Some(zlib) = zlib {
        let env_key = format!("${}", zlib.name.to_lowercase().replace("-", "_"));
        let include_path = format!("{}/include", env_key);
        let lib_path = format!("{}/lib", env_key);
        let pkgconfig_path = format!("{}/lib/pkgconfig", env_key);

        c_include_paths.push(include_path.clone());
        cppflags_args.push(format!("-I{}", include_path));
        ld_library_paths.push(lib_path.clone());
        ldflags_args.push(format!("-L{}", lib_path));
        library_paths.push(lib_path.clone());

        pkg_config_paths.push(pkgconfig_path);
    }

    let c_include_paths = c_include_paths.join(":");
    let cppflags_args = cppflags_args.join(" ");
    let ld_library_paths = ld_library_paths.join(":");
    let ldflags_args = ldflags_args.join(" ");
    let library_paths = library_paths.join(":");
    let pkg_config_paths = pkg_config_paths.join(":");

    let mut c_include_path = package
        .environment
        .iter()
        .find(|env| env.key == c_include_path_key)
        .unwrap_or(&PackageEnvironment {
            key: c_include_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut cppflags = package
        .environment
        .iter()
        .find(|env| env.key == cppflags_key)
        .unwrap_or(&PackageEnvironment {
            key: cppflags_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut ldflags = package
        .environment
        .iter()
        .find(|env| env.key == ldflags_key)
        .unwrap_or(&PackageEnvironment {
            key: ldflags_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut ld_library_path = package
        .environment
        .iter()
        .find(|env| env.key == ld_library_path_key)
        .unwrap_or(&PackageEnvironment {
            key: ld_library_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut library_path = package
        .environment
        .iter()
        .find(|env| env.key == library_path_key)
        .unwrap_or(&PackageEnvironment {
            key: library_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    let mut pkg_config_path = package
        .environment
        .iter()
        .find(|env| env.key == pkg_config_path_key)
        .unwrap_or(&PackageEnvironment {
            key: pkg_config_path_key.clone(),
            value: "".to_string(),
        })
        .clone();

    if !c_include_path.value.is_empty() {
        c_include_path.value.insert(c_include_path.value.len(), ':');
    }

    if !cppflags.value.is_empty() {
        cppflags.value.insert(cppflags.value.len(), ' ');
    }

    if !ld_library_path.value.is_empty() {
        ld_library_path
            .value
            .insert(ld_library_path.value.len(), ':');
    }

    if !ldflags.value.is_empty() {
        ldflags.value.insert(ldflags.value.len(), ' ');
    }

    if !library_path.value.is_empty() {
        library_path.value.insert(library_path.value.len(), ':');
    }

    if !pkg_config_path.value.is_empty() {
        pkg_config_path
            .value
            .insert(pkg_config_path.value.len(), ':');
    }

    c_include_path
        .value
        .insert_str(c_include_path.value.len(), c_include_paths.as_str());

    cppflags
        .value
        .insert_str(cppflags.value.len(), cppflags_args.as_str());

    ld_library_path
        .value
        .insert_str(ld_library_path.value.len(), ld_library_paths.as_str());

    ldflags
        .value
        .insert_str(ldflags.value.len(), ldflags_args.as_str());

    library_path
        .value
        .insert_str(library_path.value.len(), library_paths.as_str());

    pkg_config_path
        .value
        .insert_str(pkg_config_path.value.len(), pkg_config_paths.as_str());

    environment.push(c_include_path);
    environment.push(cppflags);
    environment.push(ld_library_path);
    environment.push(ldflags);
    environment.push(library_path);
    environment.push(pkg_config_path);

    Package {
        environment,
        name: package.name,
        packages: package.packages,
        sandbox: package.sandbox,
        script: package.script,
        source: package.source,
        systems: package.systems,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn add_default_packages(
    package: Package,
    target: PackageSystem,
    bash: &PackageOutput,
    binutils: Option<&PackageOutput>,
    coreutils: &PackageOutput,
    diffutils: Option<&PackageOutput>,
    file: Option<&PackageOutput>,
    findutils: Option<&PackageOutput>,
    gawk: Option<&PackageOutput>,
    gcc: Option<&PackageOutput>,
    glibc: Option<&PackageOutput>,
    grep: Option<&PackageOutput>,
    gzip: Option<&PackageOutput>,
    libstdcpp: Option<&PackageOutput>,
    linux_headers: Option<&PackageOutput>,
    m4: Option<&PackageOutput>,
    ncurses: Option<&PackageOutput>,
    patchelf: Option<&PackageOutput>,
    zlib: Option<&PackageOutput>,
) -> Result<Package> {
    let mut packages = vec![bash.clone(), coreutils.clone()];

    if target == Aarch64Linux || target == X8664Linux {
        if let Some(binutils) = binutils {
            packages.push(binutils.clone());
        }

        if let Some(diffutils) = diffutils {
            packages.push(diffutils.clone());
        }

        if let Some(file) = file {
            packages.push(file.clone());
        }

        if let Some(findutils) = findutils {
            packages.push(findutils.clone());
        }

        if let Some(gawk) = gawk {
            packages.push(gawk.clone());
        }

        if let Some(gcc) = gcc {
            packages.push(gcc.clone());
        }

        if let Some(glibc) = glibc {
            packages.push(glibc.clone());
        }

        if let Some(grep) = grep {
            packages.push(grep.clone());
        }

        if let Some(gzip) = gzip {
            packages.push(gzip.clone());
        }

        if let Some(libstdcpp) = libstdcpp {
            packages.push(libstdcpp.clone());
        }

        if let Some(linux_headers) = linux_headers {
            packages.push(linux_headers.clone());
        }

        if let Some(m4) = m4 {
            packages.push(m4.clone());
        }

        if let Some(ncurses) = ncurses {
            packages.push(ncurses.clone());
        }

        if let Some(patchelf) = patchelf {
            packages.push(patchelf.clone());
        }

        if let Some(zlib) = zlib {
            packages.push(zlib.clone());
        }
    }

    for package in package.packages {
        packages.push(package);
    }

    let mut script = formatdoc! {"
        #!${bash}/bin/bash
        set -euo pipefail
        export LC_ALL=\"C\"",
        bash = bash.name.to_lowercase().replace("-", "_"),
    };

    if package.script.is_empty() {
        bail!("Package script is empty");
    }

    script.push_str(format!("\n\n{}", package.script).as_str());

    Ok(Package {
        environment: package.environment,
        name: package.name,
        packages,
        sandbox: package.sandbox,
        script,
        source: package.source,
        systems: package.systems,
    })
}

pub fn add_default_script(
    package: Package,
    system: PackageSystem,
    glibc: Option<&PackageOutput>,
) -> Result<Package> {
    let mut script = package.script.clone();

    let script_paths = formatdoc! {"
        find \"$output\" -type f | while read -r file; do
            if file \"$file\" | grep -q 'interpreter'; then
                pkg_rpath=\"$(patchelf --print-rpath \"$file\")\"
                pkg_rpath_new=\"\"

                for pkg in $packages; do
                    if [ -d \"$pkg/lib\" ]; then
                        pkg_rpath_new=\"$pkg_rpath_new:$pkg/lib\"
                    fi

                    if [ -d \"$pkg/lib64\" ]; then
                        pkg_rpath_new=\"$pkg_rpath_new:$pkg/lib64\"
                    fi
                done

                if [ -d \"$output/lib\" ]; then
                    pkg_rpath_new=\"$pkg_rpath_new:${envkey}/lib\"
                fi

                if [ -d \"$output/lib64\" ]; then
                    pkg_rpath_new=\"$pkg_rpath_new:${envkey}/lib64\"
                fi

                if [ \"$pkg_rpath_new\" != \"\" ]; then
                    patchelf --set-rpath \"$pkg_rpath_new\" \"$file\"
                fi
            fi

            if file \"$file\" | grep -q 'text'; then
                {sed} \"s|$output|${envkey}|g\" \"$file\"
                {sed} \"s|$PWD|${envkey}|g\" \"$file\"
            fi
        done",
        envkey = package.name.to_lowercase().replace("-", "_"),
        sed = get_sed_cmd(system)?,
    };

    script.push_str(format!("\n\n{}", script_paths).as_str());

    if let Some(glibc) = glibc {
        let script_arch = match system {
            Aarch64Linux => "aarch64",
            X8664Linux => "x86_64",
            _ => bail!("Unsupported interpreter system"),
        };

        let script_glibc = formatdoc! {"
            find \"$output\" -type f | while read -r file; do
                if file \"$file\" | grep -q 'interpreter'; then
                    \"patchelf\" --set-interpreter \"${glibc}/lib/ld-linux-{arch}.so.1\" \"$file\"
                fi
            done",
            arch = script_arch,
            glibc = glibc.name.to_lowercase().replace("-", "_"),
        };

        script.push_str(format!("\n\n{}", script_glibc).as_str());
    }

    Ok(Package {
        environment: package.environment,
        name: package.name,
        packages: package.packages,
        sandbox: package.sandbox,
        script,
        source: package.source,
        systems: package.systems,
    })
}

pub fn build_package(
    context: &mut ContextConfig,
    package: Package,
    target: PackageSystem,
) -> Result<PackageOutput> {
    let mut package = package.clone();

    let mut bash = bash_stage_01::package(
        context, target, None, None, None, None, None, None, None, None,
    )?;

    let mut coreutils = coreutils_stage_01::package(
        context, target, &bash, None, None, None, None, None, None, None, None,
    )?;

    let mut binutils = None;
    let mut diffutils = None;
    let mut file = None;
    let mut findutils = None;
    let mut gawk = None;
    let mut gcc = None;
    let mut glibc = None;
    let mut grep = None;
    let mut gzip = None;
    let mut libstdcpp = None;
    let mut linux_headers = None;
    let mut m4 = None;
    let mut ncurses = None;
    let mut patchelf = None;
    let mut zlib = None;

    if target == Aarch64Linux || target == X8664Linux {
        let zlib_package = zlib_stage_01::package(context, target)?;

        let binutils_package = binutils_stage_01::package(context, target, &zlib_package)?;

        let gcc_package = gcc_stage_01::package(context, target, &binutils_package, &zlib_package)?;

        let linux_headers_package = linux_headers::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &zlib_package,
        )?;

        let glibc_package = glibc_stage_01::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &linux_headers_package,
            &zlib_package,
        )?;

        let libstdcpp_package = libstdcpp_stage_01::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &glibc_package,
            &linux_headers_package,
            &zlib_package,
        )?;

        let m4_package = m4_stage_01::package(
            context,
            target,
            &binutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &zlib_package,
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
            &zlib_package,
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
            Some(&zlib_package),
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
            Some(&zlib_package),
        )?;

        let diffutils_package = diffutils_stage_01::package(
            context,
            target,
            &bash,
            &binutils_package,
            &coreutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
            &zlib_package,
        )?;

        let file_package = file_stage_01::package(
            context,
            target,
            &bash,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
            &zlib_package,
        )?;

        let findutils_package = findutils_stage_01::package(
            context,
            target,
            &bash,
            &binutils_package,
            &coreutils_package,
            &file_package,
            &gcc_package,
            &glibc_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
            &zlib_package,
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
            &zlib_package,
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
            &zlib_package,
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
            &zlib_package,
        )?;

        let patchelf_package = patchelf_stage_01::package(
            context,
            target,
            &bash_package,
            &binutils_package,
            &coreutils_package,
            &diffutils_package,
            &findutils_package,
            &file_package,
            &gawk_package,
            &gcc_package,
            &glibc_package,
            &grep_package,
            &gzip_package,
            &libstdcpp_package,
            &linux_headers_package,
            &m4_package,
            &ncurses_package,
            &zlib_package,
        )?;

        bash = bash_package;
        coreutils = coreutils_package.clone();

        binutils = Some(binutils_package);
        diffutils = Some(diffutils_package);
        file = Some(file_package);
        findutils = Some(findutils_package);
        gawk = Some(gawk_package);
        gcc = Some(gcc_package);
        glibc = Some(glibc_package);
        grep = Some(grep_package);
        gzip = Some(gzip_package);
        libstdcpp = Some(libstdcpp_package);
        linux_headers = Some(linux_headers_package);
        m4 = Some(m4_package);
        ncurses = Some(ncurses_package);
        patchelf = Some(patchelf_package);
        zlib = Some(zlib_package);
    }

    package = add_default_environment(
        package,
        Some(&bash),
        binutils.as_ref(),
        gcc.as_ref(),
        glibc.as_ref(),
        libstdcpp.as_ref(),
        linux_headers.as_ref(),
        ncurses.as_ref(),
        zlib.as_ref(),
    );

    package = add_default_packages(
        package,
        target,
        &bash,
        binutils.as_ref(),
        &coreutils,
        diffutils.as_ref(),
        file.as_ref(),
        findutils.as_ref(),
        gawk.as_ref(),
        gcc.as_ref(),
        glibc.as_ref(),
        grep.as_ref(),
        gzip.as_ref(),
        libstdcpp.as_ref(),
        linux_headers.as_ref(),
        m4.as_ref(),
        ncurses.as_ref(),
        patchelf.as_ref(),
        zlib.as_ref(),
    )?;

    package = add_default_script(package, target, glibc.as_ref())?;

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
