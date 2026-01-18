use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{rust_toolchain, step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

#[derive(Default)]
pub struct Clippy {}

impl Clippy {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "clippy";
        let system = context.get_system();

        let source_target = rust_toolchain::target(system)?;
        let source_version = rust_toolchain::version();
        let source_path = format!(
            "https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz"
        );

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}-preview/.\" \"$VORPAL_OUTPUT\"");
        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
