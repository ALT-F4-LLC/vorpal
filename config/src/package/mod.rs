use crate::cross_platform::get_sed_cmd;
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{Package, PackageSystem};

pub mod cargo;
pub mod language;
pub mod native_bash;
pub mod native_coreutils;
pub mod native_glibc;
pub mod native_patchelf;
pub mod native_zstd;
pub mod protoc;
pub mod rust_std;
pub mod rustc;

pub fn add_default_environment(package: Package) -> Package {
    let mut environment = package.environment.clone();

    environment.insert("LC_ALL".to_string(), "C".to_string());

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

pub fn add_default_script(package: Package, system: PackageSystem) -> Result<Package> {
    let mut script = package.script.clone();

    let script_sanitize_paths = formatdoc! {"
        find \"$output\" -type f | while read -r file; do
            if file \"$file\" | grep -q 'text'; then
                {sed} \"s|$output|${envkey}|g\" \"$file\"
                {sed} \"s|$PWD|${envkey}|g\" \"$file\"
            fi
        done",
        envkey = package.name.to_lowercase().replace("-", "_"),
        sed = get_sed_cmd()?,
    };

    let script_sanitize_interpreters = formatdoc! {"
        find \"$output\" -type f -executable | while read -r file; do
            \"$patchelf\" --set-interpreter \"$glibc\" \"$file\"
        done",
    };

    script.push_str(format!("\n\n{}", script_sanitize_paths).as_str());

    if system == PackageSystem::Aarch64Linux || system == PackageSystem::X8664Linux {
        script.push_str(format!("\n\n{}", script_sanitize_interpreters).as_str());
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

pub fn add_default_packages(package: Package, system: PackageSystem) -> Result<Package> {
    let bash = native_bash::package(system)?;

    let mut packages = vec![
        bash.clone(),
        native_coreutils::package(system)?,
        native_zstd::package(system)?,
    ];

    if system == PackageSystem::Aarch64Linux || system == PackageSystem::X8664Linux {
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

#[derive(Clone, Debug)]
pub struct BuildPackageOptions {
    pub environment: bool,
    pub packages: bool,
    pub scripts: bool,
}

pub fn build_package(
    package: Package,
    system: PackageSystem,
    options: Option<BuildPackageOptions>,
) -> Result<Package> {
    let mut package = package.clone();

    if let Some(options) = options.clone() {
        if options.environment {
            package = add_default_environment(package);
        }

        if options.packages {
            package = add_default_packages(package, system)?;
        }

        if options.scripts {
            package = add_default_script(package, system)?;
        }
    }

    if options.is_none() {
        package = add_default_environment(package);
        package = add_default_packages(package, system)?;
        package = add_default_script(package, system)?;
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
