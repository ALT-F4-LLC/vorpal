use crate::{
    artifact::{step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::artifact::v0::ArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "gh";

    let target = context.get_target();

    let source_digest = match target {
        Aarch64Darwin => "f9d97acb8bc92eca98e2e1ab608050972e4c55dfa4a31001a63a0ce30ee4b545",
        Aarch64Linux => "",
        X8664Darwin => "",
        X8664Linux => "",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = match target {
        Aarch64Darwin => "macOS_arm64",
        Aarch64Linux => "linux_arm64",
        X8664Darwin => "macOS_amd64",
        X8664Linux => "linux_amd64",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_version = "2.69.0";

    let source_path = format!("https://github.com/cli/cli/releases/download/v{source_version}/gh_{source_version}_{source_target}.zip");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/{name}/gh_{source_version}_{source_target}/bin/gh\" \"$VORPAL_OUTPUT/bin/gh\"

        chmod +x \"$VORPAL_OUTPUT/bin/gh\"",
    };

    let step = step::shell(context, vec![], vec![], step_script).await?;

    ArtifactBuilder::new(name)
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
