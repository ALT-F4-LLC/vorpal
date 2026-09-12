use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{rust_toolchain, step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

/// Build-target `rustfmt` component of the Rust toolchain.
#[derive(Default)]
pub struct Rustfmt {}

impl Rustfmt {
    /// Creates a new `rustfmt` builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `rustfmt` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known Rust toolchain release, or if
    /// registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "rustfmt";
        let system = context.get_system();

        let source_target = rust_toolchain::target(system)?;
        let source_version = rust_toolchain::version();
        let source_path = format!(
            "https://sdk.vorpal.build/source/{name}-{source_version}-{source_target}.tar.gz"
        );

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = format!("cp -pr \"./source/{name}/{name}-{source_version}-{source_target}/{name}-preview/.\" \"$VORPAL_OUTPUT\"");
        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
