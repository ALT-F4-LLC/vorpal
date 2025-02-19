use crate::config::artifact::{add_artifact, ArtifactSource, ConfigContext};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSourceId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn source(context: &mut ConfigContext) -> Result<ArtifactSourceId> {
    let target = context.get_target();

    let hash = match target {
        Aarch64Linux => "42cec86acdeb62f23b8a65afaa67c2d8c8818f28d7d3ca55430e10e8027a6234",
        Aarch64Macos => "86c352c4ced8830cd92a9c85c2944eaa95ebb1e8908b3f01258962bcc94b9c14",
        X8664Linux => "9037b22b154e44366e6a03963bd5584f76381070baa9cf6a548bd2bfcd28b72e",
        X8664Macos => "123456789",
        UnknownSystem => bail!("Invalid go system: {:?}", context.get_target()),
    };

    let target = match target {
        Aarch64Linux => "linux-arm64",
        Aarch64Macos => "darwin-arm64",
        X8664Linux => "linux-386",
        X8664Macos => "darwin-amd64",
        UnknownSystem => bail!("Invalid go system: {:?}", target),
    };

    let version = "1.23.5";

    context
        .add_artifact_source(
            "go",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://go.dev/dl/go{}.{}.tar.gz", version, target),
            },
        )
        .await
}

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let name = "go";

    let source = source(context).await?;

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!("cp -prv \"./source/{name}/go/.\" \"$VORPAL_OUTPUT\""),
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
