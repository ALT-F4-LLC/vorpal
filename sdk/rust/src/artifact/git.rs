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

        // `$VORPAL_OUTPUT` is a staging directory the publish rename deletes,
        // and the worker refuses output that embeds it, so it must never be
        // the configured prefix. `RUNTIME_PREFIX` with relative resource
        // directories makes git locate libexec, templates and system config
        // from its own executable, and `DESTDIR` places the install tree.
        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT/bin\"

            pushd ./source/{name}/git-{source_version}

            ./configure --prefix=/

            make_flags=\"RUNTIME_PREFIX=YesPlease gitexecdir=libexec/git-core template_dir=share/git-core/templates sysconfdir=etc\"

            make $make_flags
            make $make_flags DESTDIR=$VORPAL_OUTPUT install",
        };

        let steps = vec![step::shell(context, &[], &[], step_script, &[]).await?];

        Artifact::new(name, steps, system::SYSTEMS)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
