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

    let source_digest = match system {
        Aarch64Darwin => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
        Aarch64Linux => "8a592a0dd590e92b1c0d77631e683fc743d1ed8158e0b093b6cfabf0685089af",
        X8664Darwin => "759e4b1cbb9b9d6146f0e372530283ac10d04fb35f1d08b026ae893d918e7590",
        X8664Linux => "d5e8fb327ea9568fd1ce2de3557740948a2168faff79c0e02e64bd9f040964d9",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_target = match system {
        Aarch64Darwin => "osx-aarch_64",
        Aarch64Linux => "linux-aarch_64",
        X8664Darwin => "osx-x86_64",
        X8664Linux => "linux-x86_64",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_version = "25.4";
    let source_path = format!("https://github.com/protocolbuffers/protobuf/releases/download/v{source_version}/protoc-{source_version}-{source_target}.zip");
    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/{name}/bin/protoc\" \"$VORPAL_OUTPUT/bin/protoc\"

        chmod +x \"$VORPAL_OUTPUT/bin/protoc\"",
    };

    let steps = vec![step::shell(context, vec![], vec![], step_script).await?];
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    ArtifactBuilder::new(name, steps, systems)
        .with_alias(format!("{name}:{source_version}"))
        .with_source(source)
        .build(context)
        .await
}
