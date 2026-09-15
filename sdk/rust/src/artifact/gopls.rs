use crate::{
    artifact::{go, language::go::Go, system},
    context::ConfigContext,
};
use anyhow::Result;

/// Build-target `gopls` language server, built from the Go tooling source tree.
#[derive(Default)]
pub struct Gopls {}

impl Gopls {
    /// Creates a builder for the `gopls` tool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `gopls` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the `Go` toolchain build fails, or if registering the source or
    /// artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "gopls";

        Go::new(name, system::SYSTEMS)
            .with_alias(format!("{name}:0.42.0"))
            .with_build_directory(name)
            .with_source(go::source_tools(name))
            .build(context)
            .await
    }
}
