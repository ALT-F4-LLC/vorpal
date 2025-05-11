use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::language::go::GoBuilder,
    context::ConfigContext,
    source::go_tools,
};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "goimports";

    let build_directory = format!("cmd/{name}");

    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    GoBuilder::new(name, systems)
        .with_build_directory(build_directory.as_str())
        .with_source(go_tools(name))
        .build(context)
        .await
}
