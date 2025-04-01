use crate::{
    artifact::{language::rust, step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "rust-src";

    let source_hash = "5f0adbae49a5442bf3389f7798cbacba92a94b7fefe7810ce00d1356a861d305";
    let source_version = rust::toolchain_version();
    let source_path = format!("https://static.rust-lang.org/dist/rust-src-{source_version}.tar.gz");
    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path)
        .with_hash(source_hash.to_string())
        .build();

    let step_script =
        format!("cp -prv \"./source/{name}/{name}-{source_version}/{name}/.\" \"$VORPAL_OUTPUT\"");
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
