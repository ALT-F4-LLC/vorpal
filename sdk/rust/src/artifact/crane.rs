use crate::{
    artifact::{language::go::Go, system, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

/// Build-target `crane` OCI image tool, built from the `go-containerregistry` source tree.
#[derive(Default)]
pub struct Crane;

impl Crane {
    /// Creates a builder for the `crane` tool.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Builds the `crane` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the `Go` toolchain build fails, or if registering the source or
    /// artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "crane";
        let version = "0.21.1";

        let source_path = format!("https://sdk.vorpal.build/source/crane-v{version}.tar.gz");
        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let build_directory = format!("./go-containerregistry-{version}");
        let build_path = format!("./cmd/{name}");

        Go::new(name, system::SYSTEMS)
            .with_alias(format!("{name}:{version}"))
            .with_build_directory(build_directory.as_str())
            .with_build_path(build_path.as_str())
            .with_source(source)
            .build(context)
            .await
    }
}
