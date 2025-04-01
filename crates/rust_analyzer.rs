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
    let name = "rust-analyzer";

    let target = context.get_target();

    let source_hash = match target {
        Aarch64Darwin => "ba92aa08cdada8fad8d772623b0522cb3d6e659a8edb9e037453fab998772a19",
        Aarch64Linux => "79fbf7077b846a4b28935fa6a22259d589baed2197c08bfc5c362f1e3f54db44",
        X8664Darwin => "123456789",
        X8664Linux => "b3d88f0ed6f77562f8376756d1b09fc7f5604aedcfac0ded2dd424c069e34ebe",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = get_toolchain_target(target)?;
    let source_version = get_toolchain_version();
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
