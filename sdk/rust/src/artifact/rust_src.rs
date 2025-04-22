use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{language::rust, step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let name = "rust-src";

    let source_digest = "5f0adbae49a5442bf3389f7798cbacba92a94b7fefe7810ce00d1356a861d305";
    let source_version = rust::toolchain_version();
    let source_path = format!("https://static.rust-lang.org/dist/rust-src-{source_version}.tar.gz");
    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script =
        format!("cp -prv \"./source/{name}/{name}-{source_version}/{name}/.\" \"$VORPAL_OUTPUT\"");
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
