use crate::{
    artifact::{
        language::rust::{toolchain_target, toolchain_version},
        step, ArtifactBuilder, ArtifactSourceBuilder,
    },
    context::ConfigContext,
};
use anyhow::{bail, Result};
use vorpal_schema::artifact::v0::ArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "clippy";

    let system = context.get_system();

    let source_digest = match system {
        Aarch64Darwin => "fe82bf19b064f6fca648b9be6a53ae210a9934023df364d669fc7c4ee5ccd485",
        Aarch64Linux => "5e0b5cb7e8655501369a6f42cb10b1c5d4711a0edfcbe44483c5234da485819d",
        X8664Darwin => "b13bdb47f1b60852b8cc2e01b9758edd43d5f6af2a8685a49e131a1ebb58341f",
        X8664Linux => "84168586980d4dfa8f385c83d66af0dcc3256668f0a3109b57712340251660f1",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_target = toolchain_target(system)?;
    let source_version = toolchain_version();
    let source_path =
        format!("https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}-preview/.\" \"$VORPAL_OUTPUT\"");
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
