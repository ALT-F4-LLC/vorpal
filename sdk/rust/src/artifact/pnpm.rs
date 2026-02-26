use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

#[derive(Default)]
pub struct Pnpm {}

impl Pnpm {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "pnpm";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "macos-arm64",
            Aarch64Linux => "linux-arm64",
            X8664Darwin => "macos-x64",
            X8664Linux => "linux-x64",
            _ => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = "10.5.2";
        let source_path = format!(
            "https://github.com/pnpm/pnpm/releases/download/v{source_version}/pnpm-{source_target}"
        );

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -pv \"$VORPAL_OUTPUT/bin\"
            cp -pv \"./source/{name}/pnpm-{source_target}\" \"$VORPAL_OUTPUT/bin/pnpm\"
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
