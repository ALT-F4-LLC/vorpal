use crate::{cross_platform::get_sed_cmd, ContextConfig};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSandboxPath,
};

pub mod cargo;
pub mod cross_toolchain;
pub mod language;
pub mod protoc;
pub mod rust_std;
pub mod rustc;
pub mod zlib;

pub fn build_package(context: &mut ContextConfig, package: Package) -> Result<PackageOutput> {
    let cross_toolchain = cross_toolchain::package(context)?;
    let cross_toolchain_envkey = cross_toolchain.name.to_lowercase().replace("-", "_");

    // TODO: build packages from toolchain instead of using toolchain

    // Setup PATH variable

    let path = PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/usr/sbin".to_string(),
    };

    let mut environment = vec![];

    for env in package.environment.clone().into_iter() {
        if env.key == path.key {
            continue;
        }

        environment.push(env);
    }

    let path_prev = package
        .environment
        .into_iter()
        .find(|env| env.key == path.key);

    if let Some(prev) = path_prev {
        environment.push(PackageEnvironment {
            key: path.key.clone(),
            value: format!("{}:{}", prev.value, path.value),
        });
    } else {
        environment.push(path);
    }

    // Setup packages

    let mut packages = vec![];

    packages.push(cross_toolchain.clone());

    for package in package.packages {
        packages.push(package);
    }

    let package = Package {
        environment,
        name: package.name,
        packages,
        sandbox: Some(PackageSandbox {
            paths: vec![
                PackageSandboxPath {
                    source: format!("${}/bin", cross_toolchain_envkey),
                    target: "/bin".to_string(),
                },
                PackageSandboxPath {
                    source: format!("${}/etc", cross_toolchain_envkey),
                    target: "/etc".to_string(),
                },
                PackageSandboxPath {
                    source: format!("${}/lib", cross_toolchain_envkey),
                    target: "/lib".to_string(),
                },
                PackageSandboxPath {
                    source: format!("${}/lib64", cross_toolchain_envkey),
                    target: "/lib64".to_string(),
                },
                PackageSandboxPath {
                    source: format!("${}/usr", cross_toolchain_envkey),
                    target: "/usr".to_string(),
                },
                PackageSandboxPath {
                    source: format!("${}/sbin", cross_toolchain_envkey),
                    target: "/sbin".to_string(),
                },
                PackageSandboxPath {
                    source: format!("${}/share", cross_toolchain_envkey),
                    target: "/share".to_string(),
                },
                PackageSandboxPath {
                    source: format!("${}/var", cross_toolchain_envkey),
                    target: "/var".to_string(),
                },
            ],
        }),
        script: formatdoc! {"
            #!${cross_toolchain}/bin/bash
            set -euo pipefail

            {script}",
            cross_toolchain = cross_toolchain_envkey,
            script = package.script,
        },
        source: package.source,
        systems: package.systems,
    };

    context.add_package(package)
}
