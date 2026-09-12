use crate::{
    api::artifact::ArtifactSystem::{
        Aarch64Darwin, Aarch64Linux, UnknownSystem, X8664Darwin, X8664Linux,
    },
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Build-target `protoc-gen-go` protobuf Go code generator plugin, fetched as a prebuilt
/// binary release.
#[derive(Default)]
pub struct ProtocGenGo {}

impl ProtocGenGo {
    /// Creates a builder for the pinned `protoc-gen-go` release.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `protoc-gen-go` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known `protoc-gen-go` release, or if
    /// registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "protoc-gen-go";
        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "darwin.arm64",
            Aarch64Linux => "linux.arm64",
            X8664Darwin => "darwin.amd64",
            X8664Linux => "linux.amd64",
            UnknownSystem => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "1.36.11";
        let source_path = format!("https://sdk.vorpal.build/source/protoc-gen-go.v{source_version}.{source_target}.tar.gz");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT/bin\"

            cp -pr \"source/protoc-gen-go/protoc-gen-go\" \"$VORPAL_OUTPUT/bin/protoc-gen-go\"

            chmod +x \"$VORPAL_OUTPUT/bin/protoc-gen-go\"",
        };

        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
