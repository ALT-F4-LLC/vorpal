use crate::{
    artifact::{step, system, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

/// Build-target `git` version control tool, built from source.
#[derive(Default)]
pub struct Git {}

impl Git {
    /// Creates a builder for the `git` tool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `git` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "git";

        let source_version = "2.53.0";

        let source_path = format!("https://sdk.vorpal.build/source/git-{source_version}.tar.gz");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT/bin\"

            pushd ./source/{name}/git-{source_version}

            ./configure --prefix=$VORPAL_OUTPUT

            make
            make install",
        };

        let steps = vec![step::shell(context, &[], &[], step_script, &[]).await?];

        Artifact::new(name, steps, system::SYSTEMS)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
