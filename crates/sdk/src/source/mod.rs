use crate::artifact::ArtifactSourceBuilder;
use vorpal_schema::artifact::v0::ArtifactSource;

pub fn go_tools(name: &str) -> ArtifactSource {
    let source_digest = "b4faf133f053f372cfe8ea3189bf035d19ca1661cb3ac1e7cd34a465de5641c2";

    let source_version = "0.29.0";

    let source_path =
        format!("https://go.googlesource.com/tools/+archive/refs/tags/v{source_version}.tar.gz");

    ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build()
}
