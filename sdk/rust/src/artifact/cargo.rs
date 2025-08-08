use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{rust_toolchain, step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "cargo";
    let system = context.get_system();

    let source_digest = match system {
        Aarch64Darwin => "7288c982cdf90e7bff441cda961ad8bb71ca011c8c14671a01f78703f19156a7",
        Aarch64Linux => "0000000000000000000000000000000000000000000000000000000000000000",
        X8664Darwin => "046747b1e1f14c76dac70bb280f0888f07fdd1b85fabb95f6aecbfa362d58bb7",
        X8664Linux => "f18f12fe86a2847168547433528623f1991a075d2af093192371e675ce90f812",
        _ => bail!("unsupported {name} system: {}", system.as_str_name()),
    };

    let source_target = rust_toolchain::target(system)?;
    let source_version = rust_toolchain::version();
    let source_path =
        format!("https://static.rust-lang.org/dist/{name}-{source_version}-{source_target}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = format!("cp -prv \"./source/{name}/{name}-{source_version}-{source_target}/{name}/.\" \"$VORPAL_OUTPUT\"");
    let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    ArtifactBuilder::new(name, steps, systems)
        .with_sources(vec![source])
        .build(context)
        .await
}
