use crate::ContextConfig;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSandbox, PackageSandboxPath, PackageSource,
    PackageSystem::{Aarch64Linux, X8664Linux},
};

pub fn package(
    context: &mut ContextConfig,
    binutils: &PackageOutput,
    gcc: &PackageOutput,
) -> Result<PackageOutput> {
    let name = "linux-headers";

    let package = Package {
        environment: vec![PackageEnvironment {
            key: "PATH".to_string(),
            value: "/bin:/sbin".to_string(),
        }],
        name: name.to_string(),
        packages: vec![binutils.clone(), gcc.clone()],
        sandbox: Some(PackageSandbox {
            paths: vec![
                PackageSandboxPath {
                    source: "/var/lib/vorpal/sandbox-rootfs/usr/bin".to_string(),
                    symlink: false,
                    target: "/bin".to_string(),
                },
                PackageSandboxPath {
                    source: "/var/lib/vorpal/sandbox-rootfs/etc".to_string(),
                    symlink: false,
                    target: "/etc".to_string(),
                },
                PackageSandboxPath {
                    source: "/var/lib/vorpal/sandbox-rootfs/usr/lib".to_string(),
                    symlink: false,
                    target: "/lib".to_string(),
                },
                PackageSandboxPath {
                    source: "/var/lib/vorpal/sandbox-rootfs/usr".to_string(),
                    symlink: false,
                    target: "/usr".to_string(),
                },
                PackageSandboxPath {
                    source: "/var/lib/vorpal/sandbox-rootfs/usr/sbin".to_string(),
                    symlink: false,
                    target: "/sbin".to_string(),
                },
            ],
        }),
        script: formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            cd ${{PWD}}/{source}

            make mrproper
            make headers

            find usr/include -type f ! -name '*.h' -delete

            mkdir -p \"$output/usr\"

            cp -rv usr/include \"$output/usr\"",
            source = name,
        },
        source: vec![PackageSource {
            excludes: vec![],
            hash: Some(
                "3fa3f4f3d010de5b9bde09d08a251fa3ef578d356d3a7a29b6784a6916ea0d50".to_string(),
            ),
            includes: vec![],
            name: name.to_string(),
            strip_prefix: true,
            uri: "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.10.8.tar.xz".to_string(),
        }],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    };

    context.add_package(package)
}
