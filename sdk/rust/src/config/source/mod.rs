use crate::config::{ArtifactSource, ConfigContext};
use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::ArtifactSourceId;

pub async fn go_tools(context: &mut ConfigContext) -> Result<ArtifactSourceId> {
    let hash = "b4faf133f053f372cfe8ea3189bf035d19ca1661cb3ac1e7cd34a465de5641c2";

    let version = "0.29.0";

    context
        .add_artifact_source(
            "go-tools",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!(
                    "https://go.googlesource.com/tools/+archive/refs/tags/v{}.tar.gz",
                    version
                ),
            },
        )
        .await
}
