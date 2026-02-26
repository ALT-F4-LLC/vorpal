use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};

#[derive(Default)]
pub struct NodeJS {}

impl NodeJS {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "nodejs";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "darwin-arm64",
            Aarch64Linux => "linux-arm64",
            X8664Darwin => "darwin-x64",
            X8664Linux => "linux-x64",
            _ => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "22.14.0";
        let source_path = format!(
            "https://nodejs.org/dist/v{source_version}/node-v{source_version}-{source_target}.tar.gz"
        );

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = format!(
            "cp -prv \"./source/{name}/node-v{source_version}-{source_target}/.\" \"$VORPAL_OUTPUT\""
        );
        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
