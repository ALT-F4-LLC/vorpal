use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{language::go::Go, protoc, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let protoc = protoc::build(context).await?;

    let name = "grpcurl";

    let source_version = "1.9.3";
    let source_path = format!(
        "https://github.com/fullstorydev/grpcurl/archive/refs/tags/v{source_version}.tar.gz"
    );

    let source = ArtifactSource::new(name, &source_path).build();

    let build_directory = format!("{name}-{source_version}");
    let build_path = format!("cmd/{name}/{name}.go");

    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    Go::new(name, systems)
        .with_alias(format!("{name}:{source_version}"))
        .with_artifacts(vec![protoc])
        .with_build_directory(build_directory.as_str())
        .with_build_path(build_path.as_str())
        .with_source(source)
        .build(context)
        .await
}
