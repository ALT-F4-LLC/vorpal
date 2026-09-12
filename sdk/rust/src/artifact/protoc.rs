use crate::{
    api::artifact::ArtifactSystem::{
        Aarch64Darwin, Aarch64Linux, UnknownSystem, X8664Darwin, X8664Linux,
    },
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Build-target `protoc` Protocol Buffers compiler, fetched as a prebuilt binary release.
#[derive(Default)]
pub struct Protoc {}

impl Protoc {
    /// Creates a builder for the pinned `protoc` release.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `protoc` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known `protoc` release, or if
    /// registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "protoc";
        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "osx-aarch_64",
            Aarch64Linux => "linux-aarch_64",
            X8664Darwin => "osx-x86_64",
            X8664Linux => "linux-x86_64",
            UnknownSystem => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "34.0";
        let source_path =
            format!("https://sdk.vorpal.build/source/protoc-{source_version}-{source_target}.zip");
        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT/bin\"

            cp -pr \"source/{name}/bin/protoc\" \"$VORPAL_OUTPUT/bin/protoc\"

            chmod +x \"$VORPAL_OUTPUT/bin/protoc\"",
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
