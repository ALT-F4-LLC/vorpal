use crate::artifact::rust_toolchain::get_rust_toolchain_target;
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSourceId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};
use vorpal_sdk::{
    artifact::{add_artifact, ArtifactSource},
    context::ConfigContext,
};

pub async fn source(
    context: &mut ConfigContext,
    target: &str,
    version: &str,
) -> Result<ArtifactSourceId> {
    let hash = match context.get_target() {
        Aarch64Linux => "d560efe018be876f2d5a9106f4b37222f0d315f52aeb12ffb0bfbfc8071fc5b1",
        Aarch64Macos => "6d636e93ec5f9a2e8a7c5bae381dc9a89808087b2eec1f987f8ed5a797fef556",
        X8664Linux => "4ae19ae088abd72073dbf6dfbe9c68f8c70a4c2aa77c018c63b099d8732464c3",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    context
        .add_artifact_source(
            "rust-std",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!(
                    "https://static.rust-lang.org/dist/rust-std-{version}-{target}.tar.gz"
                ),
            },
        )
        .await
}

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let name = "rust-std";

    let target = get_rust_toolchain_target(context.get_target())?;

    let source = source(context, &target, version).await?;

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!("cp -prv \"./source/{name}/{name}-{version}-{target}/{name}-{target}/.\" \"$VORPAL_OUTPUT\""),
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
