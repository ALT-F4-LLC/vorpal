use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{language::go::GoBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "protoc-gen-go-grpc";

    let source_digest = "eba0f83ab252cffe2c6209f894c4c8238b2473a403bbdbcb985af25140aac95d";
    let source_version = "1.70.0";
    let source_path =
        format!("https://github.com/grpc/grpc-go/archive/refs/tags/v{source_version}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let build_directory = format!("grpc-go-{source_version}/cmd/{name}");
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    GoBuilder::new(name, systems)
        .with_alias(format!("{name}:{source_version}"))
        .with_build_directory(build_directory.as_str())
        .with_source(source)
        .build(context)
        .await
}
