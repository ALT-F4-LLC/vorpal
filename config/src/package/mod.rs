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
pub mod native_patchelf;
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
    // pub glibc: bool,
}

pub fn add_default_environment(
    package: Package,
    options: Option<BuildPackageOptionsEnvironment>,
) -> Package {
    let mut environment = package.environment.clone();

    environment.insert("LC_ALL".to_string(), "C".to_string());

    let ld_library_path_key = "LD_LIBRARY_PATH".to_string();

    let mut ld_library_path = environment
        .get(&ld_library_path_key)
        .unwrap_or(&"".to_string())
        .clone();

    let mut ld_library_paths = vec![];

    if options.is_none() {
        ld_library_paths = vec![
            "$binutils_native_stage_01/lib",
            "$gcc_native_stage_01/lib",
            "$gcc_native_stage_01/lib64",
        ];
    }

    if let Some(options) = options.clone() {
        if options.binutils {
            ld_library_paths.push("$binutils_native_stage_01/lib");
        }

        if options.gcc {
            ld_library_paths.push("$gcc_native_stage_01/lib");
            ld_library_paths.push("$gcc_native_stage_01/lib64");
        }

        // if options.glibc {}
    }

    let ld_library_paths = ld_library_paths.join(":");

    if !ld_library_paths.is_empty() {
        if !ld_library_path.is_empty() {
            ld_library_path.insert(ld_library_path.len(), ':');
        }

        ld_library_path.insert_str(ld_library_path.len(), ld_library_paths.as_str());

        environment.insert(ld_library_path_key.clone(), ld_library_path);
    }

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
        packages.push(native_patchelf::package(system)?);
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
