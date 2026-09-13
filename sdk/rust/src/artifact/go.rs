use crate::{
    api,
    api::artifact::ArtifactSystem::{
        Aarch64Darwin, Aarch64Linux, UnknownSystem, X8664Darwin, X8664Linux,
    },
    artifact::{step, system, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};

/// Builds the shared source used by the Go tooling artifacts (`goimports`, `gopls`).
#[must_use]
pub fn source_tools(name: &str) -> api::artifact::ArtifactSource {
    let version = "0.42.0";

    let path = format!("https://sdk.vorpal.build/source/go-tools-v{version}.tar.gz");

    ArtifactSource::new(name, path.as_str()).build()
}

/// Build-target `go` toolchain, fetched as a prebuilt binary release.
#[derive(Default)]
pub struct Go {}

impl Go {
    /// Creates a builder for the `go` toolchain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `go` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known `go` release, or if registering the
    /// source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "go";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "darwin-arm64",
            Aarch64Linux => "linux-arm64",
            X8664Darwin => "darwin-amd64",
            X8664Linux => "linux-amd64",
            UnknownSystem => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "1.26.0";
        let source_path =
            format!("https://sdk.vorpal.build/source/go{source_version}.{source_target}.tar.gz");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = format!("cp -pr \"./source/{name}/go/.\" \"$VORPAL_OUTPUT\"");
        let steps = vec![step::shell(context, &[], &[], step_script, &[]).await?];

        Artifact::new(name, steps, system::SYSTEMS)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
