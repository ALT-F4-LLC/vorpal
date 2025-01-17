use crate::config::{
    artifact::{add_artifact, ConfigContext},
    ArtifactSource,
};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub fn get_go_toolchain_target(target: ArtifactSystem) -> Result<String> {
    let target = match target {
        Aarch64Linux => "linux-arm64",
        Aarch64Macos => "darwin-arm64",
        X8664Linux => "linux-386",
        X8664Macos => "darwin-amd64",
        UnknownSystem => bail!("Invalid go system: {:?}", target),
    };

    Ok(target.to_string())
}

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let hash = match context.get_target() {
        Aarch64Linux => "123456789",
        Aarch64Macos => "86c352c4ced8830cd92a9c85c2944eaa95ebb1e8908b3f01258962bcc94b9c14",
        X8664Linux => "123456789",
        X8664Macos => "123456789",
        UnknownSystem => bail!("Invalid go system: {:?}", context.get_target()),
    };

    let name = "go";

    let target = get_go_toolchain_target(context.get_target())?;

    let version = "1.23.5";

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!("cp -prv \"./source/{name}/go/.\" \"$VORPAL_OUTPUT\""),
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://go.dev/dl/go{}.{}.tar.gz", version, target),
            },
        )]),
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
