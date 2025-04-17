use crate::{
    artifact::{language::go::GoBuilder, protoc, ArtifactSourceBuilder},
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

    let source_digest = "3db5cef04f38e71c4007ed96cc827209ae5a1b6613c710cd656a252fafcde86c";
    let source = ArtifactSourceBuilder::new(name, &source_path)
        .with_digest(source_digest)
        .build();

    let build_dir = format!("{}-{}", name, source_version);
    let build_path = format!("cmd/{}/{}.go", name, name);

    GoBuilder::new(name)
        .with_artifacts(vec![protoc])
        .with_build_dir(build_dir.as_str())
        .with_build_path(build_path.as_str())
        .with_source(source)
        .build(context)
        .await
}
