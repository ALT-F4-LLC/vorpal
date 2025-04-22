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
        Aarch64Darwin => "e88e4babfc20e0546fe28bc2ba3f71a467f83e9fb1be76c9a078d327379ee4d0",
        Aarch64Linux => "42781c7ae909a5cd01c955cb4343754ce33d75783b2599a3f1a3b3752a0947af",
        X8664Darwin => "3543ccb05e2916675ea41b90f12578fe065a1a7425d65e77a8e7fc79bf09aeb4",
        X8664Linux => "62091f43974e3e24583cceae24db710e9bd6863f366b9a5891bd7a5aa3d8c0fd",
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
