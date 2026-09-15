use crate::{
    artifact::{language::go::Go, protoc::Protoc, system, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

/// Build-target `grpcurl` tool, built from source with a `protoc` dependency.
#[derive(Default)]
pub struct Grpcurl<'a> {
    protoc: Option<&'a str>,
}

impl<'a> Grpcurl<'a> {
    /// Creates a builder for the `grpcurl` tool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses an already-built `protoc` artifact digest instead of building one.
    #[must_use]
    pub fn with_protoc(mut self, protoc: &'a str) -> Self {
        self.protoc = Some(protoc);
        self
    }

    /// Builds the `grpcurl` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if building the `protoc` dependency (when not supplied via
    /// [`with_protoc`](Self::with_protoc)) or the `Go` toolchain build fails, or if
    /// registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let protoc = match self.protoc {
            Some(protoc) => protoc.to_string(),
            None => Protoc::new().build(context).await?,
        };

        let name = "grpcurl";

        let source_version = "1.9.3";
        let source_path =
            format!("https://sdk.vorpal.build/source/grpcurl-v{source_version}.tar.gz");

        let source = ArtifactSource::new(name, &source_path).build();

        let build_directory = format!("{name}-{source_version}");
        let build_path = format!("cmd/{name}/{name}.go");

        Go::new(name, system::SYSTEMS)
            .with_alias(format!("{name}:{source_version}"))
            .with_artifacts(vec![protoc])
            .with_build_directory(build_directory.as_str())
            .with_build_path(build_path.as_str())
            .with_source(source)
            .build(context)
            .await
    }
}
