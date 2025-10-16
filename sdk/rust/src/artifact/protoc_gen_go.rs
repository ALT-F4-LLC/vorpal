use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "protoc-gen-go";
    let system = context.get_system();

    let source_target = match system {
        Aarch64Darwin => "darwin.arm64",
        Aarch64Linux => "linux.arm64",
        X8664Darwin => "darwin.amd64",
        X8664Linux => "linux.amd64",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_version = "1.36.3";
    let source_path = format!("https://github.com/protocolbuffers/protobuf-go/releases/download/v{source_version}/protoc-gen-go.v{source_version}.{source_target}.tar.gz");

    let source = ArtifactSource::new(name, source_path.as_str()).build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/protoc-gen-go/protoc-gen-go\" \"$VORPAL_OUTPUT/bin/protoc-gen-go\"

        chmod +x \"$VORPAL_OUTPUT/bin/protoc-gen-go\"",
    };

    let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    Artifact::new(name, steps, systems)
        .with_aliases(vec![format!("{name}:{source_version}")])
        .with_sources(vec![source])
        .build(context)
        .await
}
