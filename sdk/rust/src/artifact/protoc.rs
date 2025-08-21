use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "protoc";
    let system = context.get_system();

    let source_target = match system {
        Aarch64Darwin => "osx-aarch_64",
        Aarch64Linux => "linux-aarch_64",
        X8664Darwin => "osx-x86_64",
        X8664Linux => "linux-x86_64",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_version = "25.4";
    let source_path = format!("https://github.com/protocolbuffers/protobuf/releases/download/v{source_version}/protoc-{source_version}-{source_target}.zip");
    let source = ArtifactSourceBuilder::new(name, source_path.as_str()).build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/{name}/bin/protoc\" \"$VORPAL_OUTPUT/bin/protoc\"

        chmod +x \"$VORPAL_OUTPUT/bin/protoc\"",
    };

    let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    ArtifactBuilder::new(name, steps, systems)
        .with_aliases(vec![format!("{name}:{source_version}")])
        .with_sources(vec![source])
        .build(context)
        .await
}
