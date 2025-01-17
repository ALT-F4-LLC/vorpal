use crate::config::{
    artifact::{add_artifact, ConfigContext},
    ArtifactSource,
};
use anyhow::Result;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = "5f0adbae49a5442bf3389f7798cbacba92a94b7fefe7810ce00d1356a861d305";

    let name = "rust-src";

    add_artifact(
        context,
        vec![],
        BTreeMap::new(),
        name,
        format!("cp -prv \"./source/{name}/{name}-{version}/{name}/.\" \"$VORPAL_OUTPUT\""),
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!("https://static.rust-lang.org/dist/{name}-{version}.tar.gz"),
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
