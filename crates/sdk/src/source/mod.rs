use crate::artifact::ArtifactSourceBuilder;
use vorpal_schema::artifact::v0::ArtifactSource;

pub fn go_tools() -> ArtifactSource {
    let hash = "b4faf133f053f372cfe8ea3189bf035d19ca1661cb3ac1e7cd34a465de5641c2";

    let version = "0.29.0";

    ArtifactSourceBuilder::new(
        "go-tools".to_string(),
        format!(
            "https://go.googlesource.com/tools/+archive/refs/tags/v{}.tar.gz",
            version
        ),
    )
    .with_hash(hash.to_string())
    .build()
}
