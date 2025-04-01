use anyhow::{bail, Result};
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};
use vorpal_sdk::{
    artifact::{
        language::rust::{toolchain_target, toolchain_version},
        step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder,
    },
    context::ConfigContext,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "cargo";

    let target = context.get_target();

    let source_hash = match target {
        Aarch64Darwin => "e88e4babfc20e0546fe28bc2ba3f71a467f83e9fb1be76c9a078d327379ee4d0",
        Aarch64Linux => "42781c7ae909a5cd01c955cb4343754ce33d75783b2599a3f1a3b3752a0947af",
        X8664Darwin => "123456789",
        X8664Linux => "62091f43974e3e24583cceae24db710e9bd6863f366b9a5891bd7a5aa3d8c0fd",
        _ => bail!("unsupported {name} system: {}", target.as_str_name()),
    };

    let source_target = toolchain_target(target)?;
    let source_version = toolchain_version();
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
        .await
}
