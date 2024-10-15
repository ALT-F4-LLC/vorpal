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
pub mod native_gcc;
pub mod native_glibc;
pub mod native_libstdcpp;
pub mod native_patchelf;
pub mod native_zlib;
pub mod native_zstd;
pub mod protoc;
pub mod rust_std;
pub mod rustc;

#[derive(Clone, Debug)]
pub struct BuildPackageOptions {
    pub environment: Option<BuildPackageOptionsEnvironment>,
    pub packages: bool,
    pub scripts: Option<BuildPackageOptionsScripts>,
}

#[derive(Clone, Debug)]
pub struct BuildPackageOptionsScripts {
    pub sanitize_interpreters: bool,
    pub sanitize_paths: bool,
}

#[derive(Clone, Debug)]
pub struct BuildPackageOptionsEnvironment {
    pub binutils: bool,
    pub gcc: bool,
    pub glibc: bool,
    pub zlib: bool,
}

pub fn add_default_environment(
    package: Package,
    options: Option<BuildPackageOptionsEnvironment>,
) -> Package {
    let mut environment = package.environment.clone();

    environment.insert("LC_ALL".to_string(), "C".to_string());

    let cc_key = "CC".to_string();
    let gcc_key = "GCC".to_string();
    let gcc_path = "$gcc_native_stage_01/bin/gcc".to_string();

    let c_include_path_key = "C_INCLUDE_PATH".to_string();
    let ld_library_path_key = "LD_LIBRARY_PATH".to_string();
    let ldflags_key = "LDFLAGS".to_string();
    let library_path_key = "LIBRARY_PATH".to_string();
    let pkg_config_path_key = "PKG_CONFIG_PATH".to_string();

    let mut c_include_path = environment
        .get(&c_include_path_key)
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
    let mut ld_library_paths = vec![];
    let mut ldflags_args = vec![];
    let mut library_paths = vec![];
    let mut pkg_config_paths = vec![];

    if options.is_none() {
        c_include_paths = vec![
            "$binutils_native_stage_01/include",
            "$gcc_native_stage_01/include",
            "$glibc_native_stage_01/include",
            "$zlib_native/include",
        ];

        ldflags_args = vec![
            "-L$binutils_native_stage_01/lib",
            "-L$gcc_native_stage_01/lib",
            "-L$gcc_native_stage_01/lib64",
            "-L$glibc_native_stage_01/lib",
            "-L$zlib_native/lib",
        ];

        ld_library_paths = vec![
            "$binutils_native_stage_01/lib",
            "$gcc_native_stage_01/lib",
            "$gcc_native_stage_01/lib64",
            "$glibc_native_stage_01/lib",
            "$zlib_native/lib",
        ];

        library_paths = vec![
            "$binutils_native_stage_01/lib",
            "$gcc_native_stage_01/lib",
            "$gcc_native_stage_01/lib64",
            "$glibc_native_stage_01/lib",
            "$zlib_native/lib",
        ];

        pkg_config_paths = vec!["$zlib_native/lib/pkgconfig"];

        environment.insert(cc_key.clone(), gcc_path.clone());
    }

    if let Some(options) = options.clone() {
        // TODO: sort in reverse build order

        if options.binutils {
            c_include_paths.push("$binutils_native_stage_01/include");

            ldflags_args.push("-L$binutils_native_stage_01/lib");

            ld_library_paths.push("$binutils_native_stage_01/lib");

            library_paths.push("$binutils_native_stage_01/lib");
        }

        if options.gcc {
            c_include_paths.push("$gcc_native_stage_01/include");

            ld_library_paths.push("$gcc_native_stage_01/lib");
            ld_library_paths.push("$gcc_native_stage_01/lib64");

            ldflags_args.push("-L$gcc_native_stage_01/lib");
            ldflags_args.push("-L$gcc_native_stage_01/lib64");

            library_paths.push("$gcc_native_stage_01/lib");
            library_paths.push("$gcc_native_stage_01/lib64");

            environment.insert(gcc_key.clone(), gcc_path);
        }

        if options.glibc {
            c_include_paths.push("$glibc_native_stage_01/include");

            ldflags_args.push("-L$glibc_native_stage_01/lib");

            ld_library_paths.push("$glibc_native_stage_01/lib");

            library_paths.push("$glibc_native_stage_01/lib");
        }

        if options.zlib {
            c_include_paths.push("$zlib_native/include");

            ld_library_paths.push("$zlib_native/lib");

            ldflags_args.push("-L$zlib_native/lib");

            library_paths.push("$zlib_native/lib");

            pkg_config_paths.push("$zlib_native/lib/pkgconfig");
        }
    }

    let c_include_paths = c_include_paths.join(":");
    let ld_library_paths = ld_library_paths.join(":");
    let ldflags_args = ldflags_args.join(" ");
    let library_paths = library_paths.join(":");
    let pkg_config_paths = pkg_config_paths.join(":");

    if !c_include_path.is_empty() {
        c_include_path.insert(c_include_path.len(), ':');
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

    ld_library_path.insert_str(ld_library_path.len(), ld_library_paths.as_str());

    ldflags.insert_str(ldflags.len(), ldflags_args.as_str());

    library_path.insert_str(library_path.len(), library_paths.as_str());

    pkg_config_path.insert_str(pkg_config_path.len(), pkg_config_paths.as_str());

    environment.insert(c_include_path_key.clone(), c_include_path);
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

pub fn add_default_packages(package: Package, system: PackageSystem) -> Result<Package> {
    let bash = native_bash::package(system)?;

    let mut packages = vec![
        bash.clone(),
        native_coreutils::package(system)?,
        native_zstd::package(system)?,
    ];

    if system == Aarch64Linux || system == X8664Linux {
        packages.push(linux_headers::package(system)?);
        packages.push(native_binutils::package(system)?);
        packages.push(native_gcc::package(system)?);
        packages.push(native_glibc::package(system)?);
        packages.push(native_libstdcpp::package(system)?);
        packages.push(native_patchelf::package(system)?);
        packages.push(native_zlib::package(system)?);
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
    options: Option<BuildPackageOptionsScripts>,
) -> Result<Package> {
    let mut script = package.script.clone();
    let mut sanitize_interpreters = String::new();

    if system == Aarch64Linux || system == X8664Linux {
        let sanitize_arch = match system {
            Aarch64Linux => "aarch64",
            X8664Linux => "x86_64",
            _ => bail!("Unsupported intrepreter system"),
        };

        sanitize_interpreters = formatdoc! {"
            find \"$output\" -type f -executable | while read -r file; do
                \"patchelf\" --set-interpreter \"$glibc_native_stage_01/lib/ld-linux-{arch}.so.1\" \"$file\" || true
            done",
            arch = sanitize_arch,
        };
    }

    let sanitize_paths = formatdoc! {"
        find \"$output\" -type f | while read -r file; do
            if file \"$file\" | grep -q 'text'; then
                {sed} \"s|$output|${envkey}|g\" \"$file\"
                {sed} \"s|$PWD|${envkey}|g\" \"$file\"
            fi
        done",
        envkey = package.name.to_lowercase().replace("-", "_"),
        sed = get_sed_cmd(system)?,
    };

    if let Some(options) = options.clone() {
        if options.sanitize_paths {
            script.push_str(format!("\n\n{}", sanitize_paths).as_str());
        }

        if options.sanitize_interpreters {
            if sanitize_interpreters.is_empty() {
                bail!("Sanitize interpreters is empty");
            }

            script.push_str(format!("\n\n{}", sanitize_interpreters).as_str());
        }
    }

    if options.is_none() {
        script.push_str(format!("\n\n{}", sanitize_paths).as_str());

        if system == Aarch64Linux || system == X8664Linux {
            if sanitize_interpreters.is_empty() {
                bail!("Sanitize interpreters is empty");
            }

            script.push_str(format!("\n\n{}", sanitize_interpreters).as_str());
        }
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
    package: Package,
    system: PackageSystem,
    options: Option<BuildPackageOptions>,
) -> Result<Package> {
    let mut package = package.clone();

    if let Some(options) = options.clone() {
        package = match options.environment {
            Some(opts) => add_default_environment(package, Some(opts)),
            None => add_default_environment(package, None),
        };

        if options.packages {
            package = add_default_packages(package, system)?;
        }

        package = match options.scripts {
            Some(opts) => add_default_script(package, system, Some(opts))?,
            None => add_default_script(package, system, None)?,
        };
    }

    if options.is_none() {
        package = add_default_environment(package, None);
        package = add_default_packages(package, system)?;
        package = add_default_script(package, system, None)?;
    }

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
