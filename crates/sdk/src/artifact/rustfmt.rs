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
    let name = "rustfmt";

    let target = context.get_target();

    let source_hash = match target {
        Aarch64Darwin => "4feacdd0fe93196c893a48458f4c3b78bf50a515b2a37a8dd03ce8ba0ef3e065",
        Aarch64Linux => "8a51bcfb496489a5fd6f2042617e84a35301d69325ce558e23589371729c75b2",
        X8664Darwin => "123456789",
        X8664Linux => "a2a4d35eeb4acb7baddb3b3974d1d08d600b135e2a67c291d585d6707f63279a",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = toolchain_target(target)?;
    let source_version = toolchain_version();
    let source_path =
        format!("https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz");

    let source = ArtifactSourceBuilder::new(name.to_string(), source_path)
        .with_hash(source_hash.to_string())
        .build();

    let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}-preview/.\" \"$VORPAL_OUTPUT\"");
    let step = step::shell(context, vec![], vec![], step_script).await?;

    ArtifactBuilder::new(name.to_string())
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
