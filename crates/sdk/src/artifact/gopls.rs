use crate::{artifact::language::go::GoBuilder, context::ConfigContext, source::go_tools};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "gopls";

    GoBuilder::new(name)
        .with_build_dir(name)
        .with_source(go_tools(name))
        .build(context)
        .await
}
