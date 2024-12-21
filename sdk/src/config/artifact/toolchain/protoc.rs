use crate::config::{artifact::add_artifact, ArtifactSource, ConfigContext};
use anyhow::{bail, Result};
use indoc::formatdoc;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "8a592a0dd590e92b1c0d77631e683fc743d1ed8158e0b093b6cfabf0685089af",
        Aarch64Macos => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
        X8664Linux => "d5e8fb327ea9568fd1ce2de3557740948a2168faff79c0e02e64bd9f040964d9",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let name = "protoc";

    let target = match context.get_target() {
        Aarch64Linux => "linux-aarch_64",
        Aarch64Macos => "osx-aarch_64",
        X8664Linux => "linux-x86_64",
        X8664Macos => "osx-x86_64",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    let version = "25.4";

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        formatdoc! {"
            mkdir -pv \"$VORPAL_OUTPUT/bin\"

            cp -prv \"source/{name}/bin/protoc\" \"$VORPAL_OUTPUT/bin/protoc\"

            chmod +x \"$VORPAL_OUTPUT/bin/protoc\"",
        },
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://github.com/protocolbuffers/protobuf/releases/download/v{version}/{name}-{version}-{target}.zip"),
            }
        )]),
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ]
    ).await
}
