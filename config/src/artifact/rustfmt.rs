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
        Aarch64Linux => "8a51bcfb496489a5fd6f2042617e84a35301d69325ce558e23589371729c75b2",
        Aarch64Macos => "4feacdd0fe93196c893a48458f4c3b78bf50a515b2a37a8dd03ce8ba0ef3e065",
        X8664Linux => "a2a4d35eeb4acb7baddb3b3974d1d08d600b135e2a67c291d585d6707f63279a",
        X8664Macos => "1234567890",
        UnknownSystem => bail!("Invalid protoc system: {:?}", context.get_target()),
    };

    context
        .add_artifact_source(
            "rustfmt",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!(
                    "https://static.rust-lang.org/dist/rustfmt-{version}-{target}.tar.gz"
                ),
            },
        )
        .await
}

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let name = "rustfmt";

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
