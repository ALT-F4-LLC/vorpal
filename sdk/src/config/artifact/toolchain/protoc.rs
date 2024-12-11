use crate::config::{artifact::add_artifact, ConfigContext};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    add_artifact(
        context,
        vec![],
        vec![],
        "protoc",
        formatdoc! {"
            curl -L -o ./protoc-{version}-{target}.zip \
                https://github.com/protocolbuffers/protobuf/releases/download/v{version}/protoc-{version}-{target}.zip

            unzip ./protoc-{version}-{target}.zip -d $VORPAL_OUTPUT

            chmod +x \"$VORPAL_OUTPUT/bin/protoc\"",
            target = match context.get_target() {
                Aarch64Linux => "linux-aarch_64",
                Aarch64Macos => "osx-aarch_64",
                X8664Linux => "linux-x86_64",
                X8664Macos => "osx-x86_64",
                UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
            },
            version = "25.4",
        },
        vec![],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ]
    ).await
}
