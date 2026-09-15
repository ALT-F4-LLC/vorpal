use crate::{
    api::artifact::ArtifactSystem::{
        Aarch64Darwin, Aarch64Linux, UnknownSystem, X8664Darwin, X8664Linux,
    },
    artifact::{step, system, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Canonical pin for the build-target `bun` runtime.
pub const DEFAULT_BUN_VERSION: &str = "1.3.10";

/// Build-target `bun` JavaScript runtime, fetched as a prebuilt binary release.
pub struct Bun {
    version: String,
}

impl Default for Bun {
    fn default() -> Self {
        Self {
            version: DEFAULT_BUN_VERSION.to_string(),
        }
    }
}

impl Bun {
    /// Creates a builder pinned to [`DEFAULT_BUN_VERSION`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the `bun` release version to fetch.
    #[must_use]
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    /// Builds the `bun` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known `bun` release, or if registering
    /// the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "bun";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "darwin-aarch64",
            Aarch64Linux => "linux-aarch64",
            X8664Darwin => "darwin-x64",
            X8664Linux => "linux-x64-baseline",
            UnknownSystem => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = &self.version;
        let source_path =
            format!("https://sdk.vorpal.build/source/bun-{source_version}-{source_target}.zip");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT/bin\"
            cp -p \"./source/{name}/bun-{source_target}/bun\" \"$VORPAL_OUTPUT/bin/bun\"
            chmod +x \"$VORPAL_OUTPUT/bin/bun\"
        "};
        let steps = vec![step::shell(context, &[], &[], step_script, &[]).await?];

        Artifact::new(name, steps, system::SYSTEMS)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
