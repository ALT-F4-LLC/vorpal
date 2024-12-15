use crate::config::artifact::{add_artifact, ConfigContext};
use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::{ArtifactId, ArtifactSource};

pub async fn artifact(context: &mut ConfigContext, version: &str) -> Result<ArtifactId> {
    let hash = "e23cc249a095345e3ba2bb05decd593e96d0024e8dad25e320cb91dfd44119af";

    let name = "rust-src";

    add_artifact(
        context,
        vec![],
        vec![],
        name,
        format!("cp -prv \"./source/{name}/{name}-{version}/{name}/.\" \"$VORPAL_OUTPUT\""),
        vec![ArtifactSource {
            excludes: vec![],
            hash: Some(hash.to_string()),
            includes: vec![],
            name: name.to_string(),
            path: format!("https://static.rust-lang.org/dist/{name}-{version}.tar.gz"),
        }],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
