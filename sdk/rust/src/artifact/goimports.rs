use crate::{artifact::language::go::GoBuilder, context::ConfigContext, source::go_tools};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "goimports";

    let build_path = format!("cmd/{name}");

    GoBuilder::new(name)
        .with_build_directory(build_path.as_str())
        .with_source(go_tools(name))
        .build(context)
        .await
}
