use crate::{cross_platform::get_sed_cmd, ContextConfig};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{Package, PackageEnvironment, PackageOutput};

pub mod cargo;
pub mod cross_toolchain;
pub mod cross_toolchain_rootfs;
pub mod language;
pub mod protoc;
pub mod rust_std;
pub mod rustc;
pub mod zlib;

pub fn build_package(context: &mut ContextConfig, package: Package) -> Result<PackageOutput> {
    let cross_toolchain_rootfs = cross_toolchain_rootfs::package(context)?;

    let cross_toolchain = cross_toolchain::package(context, &cross_toolchain_rootfs)?;
    let cross_toolchain_envkey = cross_toolchain.name.to_lowercase().replace("-", "_");

    // TODO: build packages from toolchain instead of using toolchain

    // Setup PATH variable

    let path = PackageEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/usr/sbin".to_string(),
    };

    let mut environments = vec![];

    for env in package.environments.clone().into_iter() {
        if env.key == path.key {
            continue;
        }

        environments.push(env);
    }

    let path_prev = package
        .environments
        .into_iter()
        .find(|env| env.key == path.key);

    if let Some(prev) = path_prev {
        environments.push(PackageEnvironment {
            key: path.key.clone(),
            value: format!("{}:{}", prev.value, path.value),
        });
    } else {
        environments.push(path);
    }

    // Setup packages

    let mut packages = vec![];

    packages.push(cross_toolchain.clone());

    for package in package.packages {
        packages.push(package);
    }

    let package = Package {
        environments,
        name: package.name,
        packages,
        sandbox: package.sandbox,
        script: formatdoc! {"
            #!${cross_toolchain}/bin/bash
            set -euo pipefail

            {script}",
            cross_toolchain = cross_toolchain_envkey,
            script = package.script,
        },
        sources: package.sources,
        systems: package.systems,
    };

    context.add_package(package)
}
