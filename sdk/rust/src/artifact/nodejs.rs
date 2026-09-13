use crate::{
    api::artifact::ArtifactSystem::{
        Aarch64Darwin, Aarch64Linux, UnknownSystem, X8664Darwin, X8664Linux,
    },
    artifact::{step, system, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};

/// Build-target `Node.js` JavaScript runtime, fetched as a prebuilt binary release.
#[derive(Default)]
pub struct NodeJS {}

impl NodeJS {
    /// Creates a builder for the pinned `Node.js` release.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `Node.js` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known `Node.js` release, or if
    /// registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "nodejs";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "darwin-arm64",
            Aarch64Linux => "linux-arm64",
            X8664Darwin => "darwin-x64",
            X8664Linux => "linux-x64",
            UnknownSystem => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "22.22.0";
        let source_path = format!(
            "https://sdk.vorpal.build/source/node-v{source_version}-{source_target}.tar.gz"
        );

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = format!(
            "cp -pr \"./source/{name}/node-v{source_version}-{source_target}/.\" \"$VORPAL_OUTPUT\""
        );
        let steps = vec![step::shell(context, &[], &[], step_script, &[]).await?];

        Artifact::new(name, steps, system::SYSTEMS)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
