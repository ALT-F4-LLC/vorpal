use crate::{
    artifact::{language::go::GoBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "staticcheck";

    let source_digest = "e8f40ddbc450bf26d1855916519283f7c997ffedbcb971e2a7b92892650d92b6";
    let source_version = "2025.1.1";
    let source_path =
        format!("https://github.com/dominikh/go-tools/archive/refs/tags/{source_version}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let build_dir = format!("go-tools-{}", source_version);
    let build_path = format!("cmd/{}/{}.go", name, name);

    GoBuilder::new(name)
        .with_build_dir(build_dir.as_str())
        .with_build_path(build_path.as_str())
        .with_source(source)
        .build(context)
        .await
}
