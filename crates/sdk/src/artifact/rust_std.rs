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
    let name = "rust-std";

    let system = context.get_system();

    let source_digest = match system {
        Aarch64Darwin => "6d636e93ec5f9a2e8a7c5bae381dc9a89808087b2eec1f987f8ed5a797fef556",
        Aarch64Linux => "d560efe018be876f2d5a9106f4b37222f0d315f52aeb12ffb0bfbfc8071fc5b1",
        X8664Darwin => "090a6f2f62c3f324afab5b5425856c5c1e98d433a93398b1f502b6de86cf2514",
        X8664Linux => "4ae19ae088abd72073dbf6dfbe9c68f8c70a4c2aa77c018c63b099d8732464c3",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_target = toolchain_target(system)?;
    let source_version = toolchain_version();
    let source_path =
        format!("https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}-{source_target}/.\" \"$VORPAL_OUTPUT\"");
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
