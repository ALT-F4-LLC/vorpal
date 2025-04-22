use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{
        language::rust::{toolchain_target, toolchain_version},
        step, ArtifactBuilder, ArtifactSourceBuilder,
    },
    context::ConfigContext,
};
use anyhow::{bail, Result};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "rust-analyzer";

    let system = context.get_system();

    let source_digest = match system {
        Aarch64Darwin => "ba92aa08cdada8fad8d772623b0522cb3d6e659a8edb9e037453fab998772a19",
        Aarch64Linux => "79fbf7077b846a4b28935fa6a22259d589baed2197c08bfc5c362f1e3f54db44",
        X8664Darwin => "13a085c4672e6c4e2aaaf325c1bf4d333c52283c63bb31d853386cafec573e27",
        X8664Linux => "b3d88f0ed6f77562f8376756d1b09fc7f5604aedcfac0ded2dd424c069e34ebe",
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
