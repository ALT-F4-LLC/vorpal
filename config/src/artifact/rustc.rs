use anyhow::{bail, Result};
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};
use vorpal_sdk::{
    artifact::{
        language::rust::{get_toolchain_target, get_toolchain_version},
        step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder,
    },
    context::ConfigContext,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "rustc";

    let target = context.get_target();

    let source_hash = match target {
        Aarch64Darwin => "d022dd6d61a7039c12834f90a0a5410c884bfb9ef1e38b085ad4d3f59a5bf04a",
        Aarch64Linux => "f5e5eac428b2a62ffc14324e3a6e171fb3032921f24973b27959834e456388b1",
        X8664Darwin => "123456789",
        X8664Linux => "fb18b7bb9dd94a5eeb445af1e4dd636836b6034f5dc731d534548bf5f9cb3d6f",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = get_toolchain_target(target)?;
    let source_version = get_toolchain_version();
    let source_path =
        format!("https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz");

    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path)
        .with_hash(source_hash.to_string())
        .build();

    let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}/.\" \"$VORPAL_OUTPUT\"");
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
