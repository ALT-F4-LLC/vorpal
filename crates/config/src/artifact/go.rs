use anyhow::{bail, Result};
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};
use vorpal_sdk::{
    artifact::{step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder},
    context::ConfigContext,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "go";

    let target = context.get_target();

    let source_hash = match target {
        Aarch64Darwin => "86c352c4ced8830cd92a9c85c2944eaa95ebb1e8908b3f01258962bcc94b9c14",
        Aarch64Linux => "42cec86acdeb62f23b8a65afaa67c2d8c8818f28d7d3ca55430e10e8027a6234",
        X8664Darwin => "123456789",
        X8664Linux => "9037b22b154e44366e6a03963bd5584f76381070baa9cf6a548bd2bfcd28b72e",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = match target {
        Aarch64Darwin => "darwin-arm64",
        Aarch64Linux => "linux-arm64",
        X8664Darwin => "darwin-amd64",
        X8664Linux => "linux-386",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_version = "1.23.5";
    let source_path = format!("https://go.dev/dl/go{source_version}.{source_target}.tar.gz");

    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path)
        .with_hash(source_hash.to_string())
        .build();

    let step_script = format!("cp -prv \"./source/{name}/go/.\" \"$VORPAL_OUTPUT\"");
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
