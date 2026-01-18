use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{go, language::go::Go},
    context::ConfigContext,
};
use anyhow::Result;

#[derive(Default)]
pub struct Gopls {}

impl Gopls {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "gopls";

        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Go::new(name, systems)
            .with_alias(format!("{name}:0.29.0"))
            .with_build_directory(name)
            .with_source(go::source_tools(name))
            .build(context)
            .await
    }
}
