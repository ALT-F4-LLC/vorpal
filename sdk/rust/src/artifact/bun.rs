use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

#[derive(Default)]
pub struct Bun {}

impl Bun {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "bun";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "darwin-aarch64",
            Aarch64Linux => "linux-aarch64",
            X8664Darwin => "darwin-x64",
            X8664Linux => "linux-x64",
            _ => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "1.2.0";
        let source_path = format!("https://github.com/oven-sh/bun/releases/download/bun-v{source_version}/bun-{source_target}.zip");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -pv \"$VORPAL_OUTPUT/bin\"
            cp -pv \"./source/{name}/bun-{source_target}/bun\" \"$VORPAL_OUTPUT/bin/bun\"
            chmod +x \"$VORPAL_OUTPUT/bin/bun\"
        "};
        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
