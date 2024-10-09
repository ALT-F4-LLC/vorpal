use crate::{build_package, cross_platform::get_cpu_count};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{Package, PackageSource, PackageSystem};

pub fn package() -> Result<Package> {
    let name = "glibc";

    let script_install = formatdoc! {"
        mkdir -p ${{PWD}}/{source}-build
        cd ${{PWD}}/{source}-build
        ../{source}/configure --prefix=\"$output\" libc_cv_slibdir=\"$output/lib\"
        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count()?
    };

    let source_bash = PackageSource {
        excludes: vec![],
        hash: Some("da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/glibc/glibc-2.40.tar.gz".to_string(),
    };

    let package = Package {
        environment: HashMap::new(),
        name: name.to_string(),
        packages: vec![],
        sandbox: false,
        script: HashMap::from([("install".to_string(), script_install)]),
        source: HashMap::from([(name.to_string(), source_bash)]),
        systems: vec![
            PackageSystem::Aarch64Linux.into(),
            PackageSystem::Aarch64Macos.into(),
            PackageSystem::X8664Linux.into(),
            PackageSystem::X8664Macos.into(),
        ],
    };

    build_package(package)
}
