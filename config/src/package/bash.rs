use crate::{cross_platform::get_cpu_count, glibc, package::build_package};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::HashMap;
use vorpal_schema::vorpal::package::v0::{Package, PackageSource, PackageSystem};

pub fn package() -> Result<Package> {
    let environment = HashMap::from([("LC_ALL".to_string(), "C".to_string())]);

    let name = "bash";

    let script_install = formatdoc! {"
        cd ${{PWD}}/{source}
        ./configure --prefix=$output
        make -j$({cores})
        make install",
        source = name,
        cores = get_cpu_count()?
    };

    let source_bash = PackageSource {
        excludes: vec![],
        hash: Some("7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662".to_string()),
        includes: vec![],
        strip_prefix: true,
        uri: "https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz".to_string(),
    };

    let glibc = glibc::package()?;

    let package = Package {
        environment,
        name: name.to_string(),
        packages: vec![glibc],
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
