use crate::{
    sandbox::{environments, paths},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSandbox, PackageSource,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(context: &mut ContextConfig) -> Result<PackageOutput> {
    let name = "binutils-stage-01";

    let package = Package {
        environment: environments::add_rootfs()?,
        name: name.to_string(),
        packages: vec![],
        sandbox: Some(PackageSandbox {
            paths: paths::add_rootfs(),
        }),
        script: formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            mkdir -pv {source}/build

            cd {source}/build

            ../configure \
                --disable-nls \
                --disable-werror \
                --enable-default-hash-style=\"gnu\" \
                --enable-gprofng=\"no\" \
                --enable-new-dtags \
                --prefix=\"$output\" \
                --with-sysroot=\"$output\"

            make -j$(nproc)
            make install",
            source = name,
        },
        // TODO: explore making docker image a source
        source: vec![PackageSource {
            excludes: vec![],
            hash: Some(
                "c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528".to_string(),
            ),
            includes: vec![],
            name: name.to_string(),
            strip_prefix: true,
            uri: "https://ftp.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz".to_string(),
        }],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    context.add_package(package)
}
