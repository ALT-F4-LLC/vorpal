use crate::cross_platform::get_sed_cmd;
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageSystem,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub mod cargo;
pub mod language;
pub mod linux_headers;
pub mod native_bash;
pub mod native_binutils;
pub mod native_coreutils;
pub mod native_diffutils;
pub mod native_gcc;
pub mod native_glibc;
pub mod native_libstdcpp;
pub mod native_m4;
pub mod native_ncurses;
pub mod native_patchelf;
pub mod native_zlib;
// pub mod native_zstd;
pub mod protoc;
pub mod rust_std;
pub mod rustc;

#[allow(clippy::too_many_arguments)]
fn add_default_environment(
    package: Package,
    bash: Option<Package>,
    binutils: Option<Package>,
    gcc: Option<Package>,
    glibc: Option<Package>,
    libstdcpp: Option<Package>,
    linux_headers: Option<Package>,
    ncurses: Option<Package>,
    zlib: Option<Package>,
) -> Package {
    let mut environment = package.environment.clone();

    environment.insert("LC_ALL".to_string(), "C".to_string());

    let c_include_path_key = "C_INCLUDE_PATH".to_string();
    let cppflags_key = "CPPFLAGS".to_string();
    let ld_library_path_key = "LD_LIBRARY_PATH".to_string();
    let ldflags_key = "LDFLAGS".to_string();
    let library_path_key = "LIBRARY_PATH".to_string();
    let pkg_config_path_key = "PKG_CONFIG_PATH".to_string();

    let mut c_include_path = environment
        .get(&c_include_path_key)
        .unwrap_or(&"".to_string())
        .clone();
    let mut cppflags = environment
        .get(&cppflags_key)
        .unwrap_or(&"".to_string())
        .clone();
    let mut ldflags = environment
        .get(&ldflags_key)
        .unwrap_or(&"".to_string())
        .clone();
    let mut ld_library_path = environment
        .get(&ld_library_path_key)
        .unwrap_or(&"".to_string())
        .clone();
    let mut library_path = environment
        .get(&library_path_key)
        .unwrap_or(&"".to_string())
        .clone();
    let mut pkg_config_path = environment
        .get(&pkg_config_path_key)
        .unwrap_or(&"".to_string())
        .clone();

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

        environment.insert(cc_key.clone(), gcc_path.clone());
        environment.insert(gcc_key.clone(), gcc_path);
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

    if !c_include_path.is_empty() {
        c_include_path.insert(c_include_path.len(), ':');
    }

    if !cppflags.is_empty() {
        cppflags.insert(cppflags.len(), ' ');
    }

    if !ld_library_path.is_empty() {
        ld_library_path.insert(ld_library_path.len(), ':');
    }

    if !ldflags.is_empty() {
        ldflags.insert(ldflags.len(), ' ');
    }

    if !library_path.is_empty() {
        library_path.insert(library_path.len(), ':');
    }

    if !pkg_config_path.is_empty() {
        pkg_config_path.insert(pkg_config_path.len(), ':');
    }

    c_include_path.insert_str(c_include_path.len(), c_include_paths.as_str());
    cppflags.insert_str(cppflags.len(), cppflags_args.as_str());
    ld_library_path.insert_str(ld_library_path.len(), ld_library_paths.as_str());
    ldflags.insert_str(ldflags.len(), ldflags_args.as_str());
    library_path.insert_str(library_path.len(), library_paths.as_str());
    pkg_config_path.insert_str(pkg_config_path.len(), pkg_config_paths.as_str());

    environment.insert(c_include_path_key.clone(), c_include_path);
    environment.insert(cppflags_key.clone(), cppflags);
    environment.insert(ld_library_path_key.clone(), ld_library_path);
    environment.insert(ldflags_key.clone(), ldflags);
    environment.insert(library_path_key.clone(), library_path);
    environment.insert(pkg_config_path_key.clone(), pkg_config_path);

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
    system: PackageSystem,
    bash: Package,
    coreutils: Package,
    binutils: Option<Package>,
    diffutils: Option<Package>,
    gcc: Option<Package>,
    glibc: Option<Package>,
    libstdcpp: Option<Package>,
    linux_headers: Option<Package>,
    m4: Option<Package>,
    ncurses: Option<Package>,
    patchelf: Option<Package>,
    zlib: Option<Package>,
) -> Result<Package> {
    let mut packages = vec![
        bash.clone(),
        coreutils,
        // native_zstd::package(system)?,
    ];

    if system == Aarch64Linux || system == X8664Linux {
        if let Some(binutils) = binutils {
            packages.push(binutils);
        }

        if let Some(diffutils) = diffutils {
            packages.push(diffutils);
        }

        if let Some(gcc) = gcc {
            packages.push(gcc);
        }

        if let Some(glibc) = glibc {
            packages.push(glibc);
        }

        if let Some(libstdcpp) = libstdcpp {
            packages.push(libstdcpp);
        }

        if let Some(linux_headers) = linux_headers {
            packages.push(linux_headers);
        }

        if let Some(m4) = m4 {
            packages.push(m4);
        }

        if let Some(ncurses) = ncurses {
            packages.push(ncurses);
        }

        if let Some(patchelf) = patchelf {
            packages.push(patchelf);
        }

        if let Some(zlib) = zlib {
            packages.push(zlib);
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
    glibc: Option<Package>,
) -> Result<Package> {
    let mut script = package.script.clone();

    let script_paths = formatdoc! {"
        find \"$output\" -type f | while read -r file; do
            if file \"$file\" | grep -q 'interpreter'; then
                rpath=\"\"

                for package in \"$packages\"; do
                    if [ -d \"$package/lib64\" ]; then
                        rpath=\"$rpath:$package/lib64\"
                    fi

                    if [ -d \"$package/lib\" ]; then
                        rpath=\"$rpath:$package/lib\"
                    fi
                done

                if [[ \"$rpath\" == :* ]]; then
                    rpath=\"${{rpath:1}}\"
                fi

                if [[ \"$rpath\" == *: ]]; then
                    rpath=\"${{rpath::-1}}\"
                fi

                if [[ \"$rpath\" != \"\" ]]; then
                    \"patchelf\" --set-rpath \"$rpath\" \"$file\"
                fi
            fi

            if file \"$file\" | grep -q 'text'; then
                {sed} \"s|$output|${envkey}|g\" \"$file\"
                {sed} \"s|$PWD|${envkey}|g\" \"$file\"
            fi
        done
        ",
        envkey = package.name.to_lowercase().replace("-", "_"),
        sed = get_sed_cmd(system)?,
    };

    script.push_str(format!("\n\n{}", script_paths).as_str());

    if let Some(glibc) = glibc {
        let script_archtype = match system {
            Aarch64Linux => "aarch64",
            X8664Linux => "x86_64",
            _ => bail!("Unsupported interpreter system"),
        };

        let script_glibc = formatdoc! {"
            find \"$output\" -type f | while read -r file; do
                if file \"$file\" | grep -q 'interpreter'; then
                    \"patchelf\" --set-interpreter \"${interpreter}/lib/ld-linux-{archtype}.so.1\" \"$file\"
                fi
            done",
            archtype = script_archtype,
            interpreter = glibc.name.to_lowercase().replace("-", "_"),
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

pub fn build_package(package: Package, target: PackageSystem) -> Result<Package> {
    let mut package = package.clone();

    let mut bash = native_bash::package(target, None, None, None, None, None, None, None, None)?;

    let mut coreutils = native_coreutils::package(
        target,
        bash.clone(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    let mut binutils = None;
    let mut diffutils = None;
    let mut gcc = None;
    let mut glibc = None;
    let mut libstdcpp = None;
    let mut linux_headers = None;
    let mut m4 = None;
    let mut ncurses = None;
    let mut patchelf = None;
    let mut zlib = None;

    if target == Aarch64Linux || target == X8664Linux {
        let zlib_package = native_zlib::package(target)?;

        let binutils_package = native_binutils::package(target, zlib_package.clone())?;

        let gcc_package =
            native_gcc::package(target, binutils_package.clone(), zlib_package.clone())?;

        let linux_headers_package = linux_headers::package(
            target,
            binutils_package.clone(),
            gcc_package.clone(),
            zlib_package.clone(),
        )?;

        let glibc_package = native_glibc::package(
            target,
            binutils_package.clone(),
            gcc_package.clone(),
            linux_headers_package.clone(),
            zlib_package.clone(),
        )?;

        let libstdcpp_package = native_libstdcpp::package(
            target,
            binutils_package.clone(),
            gcc_package.clone(),
            glibc_package.clone(),
            linux_headers_package.clone(),
            zlib_package.clone(),
        )?;

        let m4_package = native_m4::package(
            target,
            binutils_package.clone(),
            gcc_package.clone(),
            glibc_package.clone(),
            libstdcpp_package.clone(),
            linux_headers_package.clone(),
            zlib_package.clone(),
        )?;

        let ncurses_package = native_ncurses::package(
            target,
            binutils_package.clone(),
            gcc_package.clone(),
            glibc_package.clone(),
            libstdcpp_package.clone(),
            linux_headers_package.clone(),
            m4_package.clone(),
            zlib_package.clone(),
        )?;

        let bash_package = native_bash::package(
            target,
            Some(binutils_package.clone()),
            Some(gcc_package.clone()),
            Some(glibc_package.clone()),
            Some(libstdcpp_package.clone()),
            Some(linux_headers_package.clone()),
            Some(m4_package.clone()),
            Some(ncurses_package.clone()),
            Some(zlib_package.clone()),
        )?;

        let coreutils_package = native_coreutils::package(
            target,
            bash_package.clone(),
            Some(binutils_package.clone()),
            Some(gcc_package.clone()),
            Some(glibc_package.clone()),
            Some(libstdcpp_package.clone()),
            Some(linux_headers_package.clone()),
            Some(m4_package.clone()),
            Some(ncurses_package.clone()),
            Some(zlib_package.clone()),
        )?;

        let diffutils_package = native_diffutils::package(
            target,
            bash.clone(),
            binutils_package.clone(),
            coreutils_package.clone(),
            gcc_package.clone(),
            glibc_package.clone(),
            libstdcpp_package.clone(),
            linux_headers_package.clone(),
            m4_package.clone(),
            ncurses_package.clone(),
            zlib_package.clone(),
        )?;

        let patchelf_package = native_patchelf::package(
            target,
            bash_package.clone(),
            binutils_package.clone(),
            coreutils_package.clone(),
            diffutils_package.clone(),
            gcc_package.clone(),
            glibc_package.clone(),
            libstdcpp_package.clone(),
            linux_headers_package.clone(),
            m4_package.clone(),
            ncurses_package.clone(),
            zlib_package.clone(),
        )?;

        bash = bash_package.clone();
        coreutils = coreutils_package.clone();

        binutils = Some(binutils_package);
        diffutils = Some(diffutils_package);
        gcc = Some(gcc_package);
        glibc = Some(glibc_package);
        libstdcpp = Some(libstdcpp_package);
        linux_headers = Some(linux_headers_package);
        m4 = Some(m4_package);
        ncurses = Some(ncurses_package);
        patchelf = Some(patchelf_package);
        zlib = Some(zlib_package);
    }

    package = add_default_environment(
        package,
        Some(bash.clone()),
        binutils.clone(),
        gcc.clone(),
        glibc.clone(),
        libstdcpp.clone(),
        linux_headers.clone(),
        ncurses.clone(),
        zlib.clone(),
    );

    package = add_default_packages(
        package,
        target,
        bash,
        coreutils,
        binutils,
        diffutils,
        gcc,
        glibc.clone(),
        libstdcpp,
        linux_headers,
        m4,
        ncurses,
        patchelf,
        zlib,
    )?;

    package = add_default_script(package, target, glibc)?;

    Ok(Package {
        environment: package.environment,
        name: package.name,
        packages: package.packages,
        sandbox: package.sandbox,
        script: package.script,
        source: package.source,
        systems: package.systems,
    })
}
