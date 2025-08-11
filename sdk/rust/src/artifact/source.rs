use crate::{api::artifact::ArtifactSource, artifact::ArtifactSourceBuilder};

pub fn go_tools(name: &str) -> ArtifactSource {

    let source_version = "0.29.0";

    let source_path =
        format!("https://go.googlesource.com/tools/+archive/refs/tags/v{source_version}.tar.gz");

    ArtifactSourceBuilder::new(name, source_path.as_str()).build()
}
