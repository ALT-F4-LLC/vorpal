use crate::{package::build_package, ContextConfig};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageOutput, PackageSource, PackageSystem,
    PackageSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub fn package(context: &mut ContextConfig, system: PackageSystem) -> Result<PackageOutput> {
    let name = "protoc";

    let script = formatdoc! {"
        cp -r ./{name}/bin \"$output/bin\"
        cp -r ./{name}/include \"$output/include\"
        chmod +x \"$output/bin/protoc\"",
        name = name,
    };

    let hash = match system {
        Aarch64Linux => "8a592a0dd590e92b1c0d77631e683fc743d1ed8158e0b093b6cfabf0685089af",
        Aarch64Macos => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
        X8664Linux => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
        X8664Macos => "",
        UnknownSystem => bail!("Unsupported system: {:?}", system),
    };

    let target = match system {
        Aarch64Linux => "linux-aarch_64",
        Aarch64Macos => "osx-aarch_64",
        X8664Linux => "linux-x86_64",
        X8664Macos => "osx-x86_64",
        UnknownSystem => bail!("Unsupported system: {:?}", system),
    };

    let version = "25.4";

    let source = PackageSource {
        excludes: vec![],
        hash: Some(hash.to_string()),
        includes: vec![],
        name: name.to_string(),
        strip_prefix: false,
        uri: format!(
            "https://github.com/protocolbuffers/protobuf/releases/download/v{}/protoc-{}-{}.zip",
            version, version, target
        ),
    };

    let package = Package {
        environment: vec![],
        name: name.to_string(),
        packages: vec![],
        sandbox: None,
        script,
        source: vec![source],
        systems: vec![
            Aarch64Linux.into(),
            Aarch64Macos.into(),
            X8664Linux.into(),
            X8664Macos.into(),
        ],
    };

    build_package(context, package, system)
}
