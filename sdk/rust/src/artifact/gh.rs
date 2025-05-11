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

    let source_digest = match system {
        Aarch64Darwin => "f9d97acb8bc92eca98e2e1ab608050972e4c55dfa4a31001a63a0ce30ee4b545",
        Aarch64Linux => "c63c8e24845de186b7130a8b9a0923ef93a8a031746dafaefae987ef04258f26",
        X8664Darwin => "08b1817823dda03bc779f9b234b7d2db7bafcd1c74b5556bc53df4552feb768b",
        X8664Linux => "27940116086dc2adaa8f12a6a12d8ca418b0613d6184f410846e1367777f4745",
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
    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/{name}/gh_{source_version}_{source_target}/bin/gh\" \"$VORPAL_OUTPUT/bin/gh\"

        chmod +x \"$VORPAL_OUTPUT/bin/gh\"",
    };

    let steps = vec![step::shell(context, vec![], vec![], step_script).await?];
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    ArtifactBuilder::new(name, steps, systems)
        .with_source(source)
        .build(context)
        .await
}
