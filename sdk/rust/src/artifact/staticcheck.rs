use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{language::go::GoBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "staticcheck";
    let source_version = "2025.1.1";
    let source_path =
        format!("https://github.com/dominikh/go-tools/archive/refs/tags/{source_version}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str()).build();

    let build_directory = format!("go-tools-{source_version}");
    let build_path = format!("cmd/{name}/{name}.go");
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    GoBuilder::new(name, systems)
        .with_alias(format!("{name}:{source_version}"))
        .with_build_directory(build_directory.as_str())
        .with_build_path(build_path.as_str())
        .with_source(source)
        .build(context)
        .await
}
