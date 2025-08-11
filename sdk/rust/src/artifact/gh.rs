use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "gh";
    let system = context.get_system();
    match system {
        Aarch64Darwin | Aarch64Linux | X8664Darwin | X8664Linux => {}
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_target = match system {
        Aarch64Darwin => "macOS_arm64",
        Aarch64Linux => "linux_arm64",
        X8664Darwin => "macOS_amd64",
        X8664Linux => "linux_amd64",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_extension = match system {
        Aarch64Darwin | X8664Darwin => "zip",
        Aarch64Linux | X8664Linux => "tar.gz",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_version = "2.69.0";
    let source_path = format!("https://github.com/cli/cli/releases/download/v{source_version}/gh_{source_version}_{source_target}.{source_extension}");
    let source = ArtifactSourceBuilder::new(name, source_path.as_str()).build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/{name}/gh_{source_version}_{source_target}/bin/gh\" \"$VORPAL_OUTPUT/bin/gh\"

        chmod +x \"$VORPAL_OUTPUT/bin/gh\"",
    };

    let step = step::shell(context, vec![], vec![], step_script, vec![]).await?;
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    ArtifactBuilder::new(name, vec![step], systems)
        .with_aliases(vec![format!("{name}:{source_version}")])
        .with_sources(vec![source])
        .build(context)
        .await
}
