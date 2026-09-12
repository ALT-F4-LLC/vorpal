use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{language::go::Go, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

/// Build-target `protoc-gen-go-grpc` protobuf gRPC Go code generator plugin, built from
/// source with the Go toolchain.
#[derive(Default)]
pub struct ProtocGenGoGrpc {}

impl ProtocGenGoGrpc {
    /// Creates a builder for the pinned `protoc-gen-go-grpc` release.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `protoc-gen-go-grpc` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if registering the source with the build context fails, or if the
    /// underlying Go build fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "protoc-gen-go-grpc";

        let source_version = "1.79.1";
        let source_path =
            format!("https://sdk.vorpal.build/source/protoc-gen-go-grpc-v{source_version}.tar.gz");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let build_directory = format!("grpc-go-{source_version}/cmd/{name}");
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Go::new(name, systems)
            .with_alias(format!("{name}:{source_version}"))
            .with_build_directory(build_directory.as_str())
            .with_source(source)
            .build(context)
            .await
    }
}
