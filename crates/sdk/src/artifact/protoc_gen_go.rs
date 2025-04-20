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
    let name = "protoc-gen-go";

    let system = context.get_system();

    let source_digest = match system {
        Aarch64Darwin => "55c2a0cc7137f3625bd1bf3be85ed940c643e56fa1ceaf51f94c6434980f65a5",
        Aarch64Linux => "597aae8080d7e3e575198a5417ac2278ae49078d7fa3be56405ffb43bbb9f501",
        X8664Darwin => "2fef5cf3ddb65da0b072b74c03bd870fbd0baf30161386880700cf787e1f3265",
        X8664Linux => "07f2ee9051854e2d240c56e47cfa9ac9b7d6a2dc2a9b2b6dbd79726f78c27bb1",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_target = match system {
        Aarch64Darwin => "darwin.arm64",
        Aarch64Linux => "linux.arm64",
        X8664Darwin => "darwin.amd64",
        X8664Linux => "linux.amd64",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_version = "1.36.3";

    let source_path = format!("https://github.com/protocolbuffers/protobuf-go/releases/download/v{source_version}/protoc-gen-go.v{source_version}.{source_target}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/protoc-gen-go/protoc-gen-go\" \"$VORPAL_OUTPUT/bin/protoc-gen-go\"

        chmod +x \"$VORPAL_OUTPUT/bin/protoc-gen-go\"",
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
