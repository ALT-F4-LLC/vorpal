use crate::config::{
    artifact::{add_artifact, language::rust::get_rust_toolchain_target, ConfigContext},
    ArtifactSource,
};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSourceId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn source(
    context: &mut ConfigContext,
    target: &str,
    version: &str,
) -> Result<ArtifactSourceId> {
    let hash = match context.get_target() {
        Aarch64Linux => "79fbf7077b846a4b28935fa6a22259d589baed2197c08bfc5c362f1e3f54db44",
        Aarch64Macos => "ba92aa08cdada8fad8d772623b0522cb3d6e659a8edb9e037453fab998772a19",
        X8664Linux => "b3d88f0ed6f77562f8376756d1b09fc7f5604aedcfac0ded2dd424c069e34ebe",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    context
        .add_artifact_source(
            "rust-analyzer",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!(
                    "https://static.rust-lang.org/dist/rust-analyzer-{version}-{target}.tar.gz"
                ),
            },
        )
        .await
}

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let name = "rust-analyzer";

    let target = get_rust_toolchain_target(context.get_target())?;

    let source = source(context, &target, version).await?;

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!("cp -prv \"./source/{name}/{name}-{version}-{target}/{name}-preview/.\" \"$VORPAL_OUTPUT\""),
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
