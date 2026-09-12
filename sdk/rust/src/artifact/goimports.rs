use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{go, language::go::Go},
    context::ConfigContext,
};
use anyhow::Result;

/// Build-target `goimports` tool, built from the Go tooling source tree.
#[derive(Default)]
pub struct Goimports {}

impl Goimports {
    /// Creates a builder for the `goimports` tool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `goimports` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the `Go` toolchain build fails, or if registering the source or
    /// artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "goimports";

        let build_directory = format!("cmd/{name}");

        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Go::new(name, systems)
            .with_alias(format!("{name}:0.42.0"))
            .with_build_directory(build_directory.as_str())
            .with_source(go::source_tools(name))
            .build(context)
            .await
    }
}
