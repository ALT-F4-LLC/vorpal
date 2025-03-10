use crate::{
    artifact::{add_artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{ArtifactId, ArtifactSourceId};

pub async fn source(context: &mut ConfigContext, version: &str) -> Result<ArtifactSourceId> {
    let hash = "5f0adbae49a5442bf3389f7798cbacba92a94b7fefe7810ce00d1356a861d305";

    context
        .add_artifact_source(
            "rust-src",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://static.rust-lang.org/dist/rust-src-{version}.tar.gz"),
            },
        )
        .await
}

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let name = "rust-src";

    let source = source(context, version).await?;

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!("cp -prv \"./source/{name}/{name}-{version}/{name}/.\" \"$VORPAL_OUTPUT\""),
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
