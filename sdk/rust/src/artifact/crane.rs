use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{language::go::Go, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

#[derive(Default)]
pub struct Crane;

impl Crane {
    pub fn new() -> Self {
        Self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "crane";
        let version = "0.20.7";

        let source_path = format!(
            "https://github.com/google/go-containerregistry/archive/refs/tags/v{version}.tar.gz"
        );
        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let build_directory = format!("./go-containerregistry-{version}");
        let build_path = format!("./cmd/{name}");

        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Go::new(name, systems)
            .with_alias(format!("{name}:{version}"))
            .with_build_directory(build_directory.as_str())
            .with_build_path(build_path.as_str())
            .with_source(source)
            .build(context)
            .await
    }
}
