use crate::{
    artifact::{
        language::rust::{toolchain_target, toolchain_version},
        step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder,
    },
    context::ConfigContext,
};
use anyhow::{bail, Result};
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "clippy";

    let target = context.get_target();

    let source_hash = match target {
        Aarch64Darwin => "fe82bf19b064f6fca648b9be6a53ae210a9934023df364d669fc7c4ee5ccd485",
        Aarch64Linux => "5e0b5cb7e8655501369a6f42cb10b1c5d4711a0edfcbe44483c5234da485819d",
        X8664Darwin => "123456789",
        X8664Linux => "84168586980d4dfa8f385c83d66af0dcc3256668f0a3109b57712340251660f1",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = toolchain_target(target)?;
    let source_version = toolchain_version();
    let source_path =
        format!("https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz");

    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path)
        .with_hash(source_hash.to_string())
        .build();

    let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}-preview/.\" \"$VORPAL_OUTPUT\"");
    let step = step::shell(context, vec![], vec![], step_script).await?;

    ConfigArtifactBuilder::new(name.to_string())
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
}
