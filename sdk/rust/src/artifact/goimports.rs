use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{go, language},
    context::ConfigContext,
};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "goimports";

    let build_directory = format!("cmd/{name}");

    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    language::go::Go::new(name, systems)
        .with_alias(format!("{name}:0.29.0"))
        .with_build_directory(build_directory.as_str())
        .with_source(go::source_tools(name))
        .build(context)
        .await
}
