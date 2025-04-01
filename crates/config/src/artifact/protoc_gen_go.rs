use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};
use vorpal_sdk::{
    artifact::{step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder},
    context::ConfigContext,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "protoc-gen-go";

    let target = context.get_target();

    let source_hash = match target {
        Aarch64Darwin => "55c2a0cc7137f3625bd1bf3be85ed940c643e56fa1ceaf51f94c6434980f65a5",
        Aarch64Linux => "597aae8080d7e3e575198a5417ac2278ae49078d7fa3be56405ffb43bbb9f501",
        X8664Darwin => "123456789",
        X8664Linux => "07f2ee9051854e2d240c56e47cfa9ac9b7d6a2dc2a9b2b6dbd79726f78c27bb1",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = match target {
        Aarch64Darwin => "darwin.arm64",
        Aarch64Linux => "linux.arm64",
        X8664Darwin => "darwin.amd64",
        X8664Linux => "linux.amd64",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_version = "1.36.3";

    let source_path = format!("https://github.com/protocolbuffers/protobuf-go/releases/download/v{source_version}/protoc-gen-go.v{source_version}.{source_target}.tar.gz");

    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path)
        .with_hash(source_hash.to_string())
        .build();

    let step_script = formatdoc! {"
        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        cp -prv \"source/protoc-gen-go/protoc-gen-go\" \"$VORPAL_OUTPUT/bin/protoc-gen-go\"

        chmod +x \"$VORPAL_OUTPUT/bin/protoc-gen-go\"",
    };

    let step = step::shell(context, vec![], vec![], step_script).await?;

    ConfigArtifactBuilder::new(name.to_string())
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
