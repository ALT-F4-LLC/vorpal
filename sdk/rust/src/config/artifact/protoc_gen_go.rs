use crate::config::{artifact::add_artifact, ArtifactSource, ConfigContext};
use anyhow::{bail, Result};
use indoc::formatdoc;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSourceId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn source(context: &mut ConfigContext) -> Result<ArtifactSourceId> {
    let hash = match context.get_target() {
        Aarch64Linux => "597aae8080d7e3e575198a5417ac2278ae49078d7fa3be56405ffb43bbb9f501",
        Aarch64Macos => "55c2a0cc7137f3625bd1bf3be85ed940c643e56fa1ceaf51f94c6434980f65a5",
        X8664Linux => "1234567890",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc-gen-go system: {:?}", context.get_target()),
    };

    let target = match context.get_target() {
        Aarch64Linux => "linux.arm64",
        Aarch64Macos => "darwin.arm64",
        X8664Linux => "linux.amd64",
        X8664Macos => "darwin.amd64",
        UnknownSystem => bail!("Invalid protoc-gen-go system: {:?}", context.get_target()),
    };

    let version = "1.36.3";

    context
        .add_artifact_source(
            "protoc-gen-go",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://github.com/protocolbuffers/protobuf-go/releases/download/v{version}/protoc-gen-go.v{version}.{target}.tar.gz"),
            },
        )
        .await
}

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let name = "protoc-gen-go";

    let source = source(context).await?;

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        formatdoc! {"
            mkdir -pv \"$VORPAL_OUTPUT/bin\"

            cp -prv \"source/protoc-gen-go/protoc-gen-go\" \"$VORPAL_OUTPUT/bin/protoc-gen-go\"

            chmod +x \"$VORPAL_OUTPUT/bin/protoc-gen-go\"",
        },
        vec![source],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
