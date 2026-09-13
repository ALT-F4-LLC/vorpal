use crate::{
    artifact::{rust_toolchain, step, system, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

/// Build-target `rust-std` component of the Rust toolchain.
#[derive(Default)]
pub struct RustStd {}

impl RustStd {
    /// Creates a new `rust-std` builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the `rust-std` artifact for the context's target system.
    ///
    /// # Errors
    ///
    /// Returns an error if the target system has no known Rust toolchain release, or if
    /// registering the source or artifact with the build context fails.
    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "rust-std";
        let system = context.get_system();

        let source_target = rust_toolchain::target(system)?;
        let source_version = rust_toolchain::version();
        let source_path = format!(
            "https://sdk.vorpal.build/source/{name}-{source_version}-{source_target}.tar.gz"
        );

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = format!("cp -pr \"./source/{name}/{name}-{source_version}-{source_target}/{name}-{source_target}/.\" \"$VORPAL_OUTPUT\"");
        let steps = vec![step::shell(context, &[], &[], step_script, &[]).await?];

        Artifact::new(name, steps, system::SYSTEMS)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
