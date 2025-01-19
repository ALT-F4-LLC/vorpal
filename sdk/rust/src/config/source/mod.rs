use crate::config::{ArtifactSource, ConfigContext};
use anyhow::{bail, Result};
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactSourceId,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};

pub async fn go_tools(context: &mut ConfigContext) -> Result<ArtifactSourceId> {
    let hash = match context.get_target() {
        Aarch64Linux => "123456789",
        Aarch64Macos => "b4faf133f053f372cfe8ea3189bf035d19ca1661cb3ac1e7cd34a465de5641c2",
        X8664Linux => "123456789",
        X8664Macos => "123456789",
        UnknownSystem => bail!("Invalid go-tools system: {:?}", context.get_target()),
    };

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
