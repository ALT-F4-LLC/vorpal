use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{rust_toolchain, step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "rust-analyzer";
    let system = context.get_system();

    let source_digest = match system {
        Aarch64Darwin => "b233955ae4c8cee649044d779044a8bec377df8d65bb0f3f75bd683454d8fe48",
        Aarch64Linux => "0000000000000000000000000000000000000000000000000000000000000000",
        X8664Darwin => "3d9ae24db261344379c3d2baa22e060fbc45cfe1322eb2cc5b38cdec3c227ab1",
        X8664Linux => "0000000000000000000000000000000000000000000000000000000000000000",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_target = rust_toolchain::target(system)?;
    let source_version = rust_toolchain::version();
    let source_path =
        format!("https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}-preview/.\" \"$VORPAL_OUTPUT\"");
    let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    ArtifactBuilder::new(name, steps, systems)
        .with_sources(vec![source])
        .build(context)
        .await
}
