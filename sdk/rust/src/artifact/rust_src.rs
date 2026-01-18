use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{rust_toolchain, step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;

#[derive(Default)]
pub struct RustSrc {}

impl RustSrc {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "rust-src";
        let source_version = rust_toolchain::version();
        let source_path =
            format!("https://static.rust-lang.org/dist/rust-src-{source_version}.tar.gz");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = format!(
            "cp -prv \"./source/{name}/{name}-{source_version}/{name}/.\" \"$VORPAL_OUTPUT\""
        );
        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
