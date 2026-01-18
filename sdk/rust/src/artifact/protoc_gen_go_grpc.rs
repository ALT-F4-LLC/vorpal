use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{language::go::Go, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

#[derive(Default)]
pub struct ProtocGenGoGrpc {}

impl ProtocGenGoGrpc {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "protoc-gen-go-grpc";

        let source_version = "1.70.0";
        let source_path =
            format!("https://github.com/grpc/grpc-go/archive/refs/tags/v{source_version}.tar.gz");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let build_directory = format!("grpc-go-{source_version}/cmd/{name}");
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Go::new(name, systems)
            .with_alias(format!("{name}:{source_version}"))
            .with_build_directory(build_directory.as_str())
            .with_source(source)
            .build(context)
            .await
    }
}
