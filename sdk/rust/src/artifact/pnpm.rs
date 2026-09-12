use crate::{
    api::artifact::ArtifactSystem::{
        Aarch64Darwin, Aarch64Linux, UnknownSystem, X8664Darwin, X8664Linux,
    },
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Build-target `pnpm` package manager, fetched as a prebuilt binary release.
#[derive(Default)]
pub struct Pnpm {}

impl Pnpm {
    /// Creates a builder for the pinned `pnpm` release.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `pnpm` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known `pnpm` release, or if
    /// registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "pnpm";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "macos-arm64",
            Aarch64Linux => "linux-arm64",
            X8664Darwin => "macos-x64",
            X8664Linux => "linux-x64",
            UnknownSystem => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "10.30.3";
        let source_path =
            format!("https://sdk.vorpal.build/source/pnpm-{source_version}-{source_target}");

        let source_filename = format!("pnpm-{source_version}-{source_target}");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT/bin\"
            cp -p \"./source/{name}/{source_filename}\" \"$VORPAL_OUTPUT/bin/pnpm\"
            chmod +x \"$VORPAL_OUTPUT/bin/pnpm\""
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
