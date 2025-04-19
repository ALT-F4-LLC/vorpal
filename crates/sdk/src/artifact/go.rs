use crate::{
    artifact::{step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use vorpal_schema::artifact::v0::ArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "go";

    let target = context.get_target();

    let source_digest = match target {
        Aarch64Darwin => "5380e02cdfe2b254af7c3306671fbacc0bfefeb3a9684b502e4af3ad5db917e7",
        Aarch64Linux => "87116daeec496cbc32774c024839ce7a7d0dfced9959fb54527bd55b8890791e",
        X8664Darwin => "b5903639cc049e527796b8c1330cec3be12ef11d15668c08a1732c03f0cf1dcd",
        X8664Linux => "78181c114c22ddf6413032d5fcc24760a3bee185c35251392fd78691975773aa",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = match target {
        Aarch64Darwin => "darwin-arm64",
        Aarch64Linux => "linux-arm64",
        X8664Darwin => "darwin-amd64",
        X8664Linux => "linux-amd64",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_version = "1.24.2";
    let source_path = format!("https://go.dev/dl/go{source_version}.{source_target}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = format!("cp -prv \"./source/{name}/go/.\" \"$VORPAL_OUTPUT\"");
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
