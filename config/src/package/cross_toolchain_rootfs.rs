use crate::ContextConfig;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSource,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(context: &mut ContextConfig) -> Result<PackageOutput> {
    let name = "cross-toolchain-rootfs";

    let package = Package {
        environments: vec![PackageEnvironment {
            key: "PATH".to_string(),
            value: "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string(),
        }],
        name: name.to_string(),
        packages: vec![],
        sandbox: None,
        sources: vec![PackageSource {
            excludes: vec![],
            hash: None,
            includes: vec![
                "Dockerfile".to_string(),
                "script/version_check.sh".to_string(),
            ],
            name: "docker".to_string(),
            path: ".".to_string(),
        }],
        script: formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')

            pushd ./docker

            docker buildx build \
                --load \
                --progress=\"plain\" \
                --tag \"altf4llc/vorpal-rootfs:latest\" \
                .

            popd

            CONTAINER_ID=$(docker container create \"altf4llc/vorpal-rootfs:latest\")

            docker export $CONTAINER_ID | gzip -v > $ARCH-export.tar.gz

            docker container rm --force $CONTAINER_ID

            tar -xvf $ARCH-export.tar.gz -C $output

            echo \"nameserver 1.1.1.1\" > $output/etc/resolv.conf",
        },
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    context.add_package(package)
}
