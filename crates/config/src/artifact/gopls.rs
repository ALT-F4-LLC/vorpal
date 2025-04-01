use crate::{artifact::go, source::go_tools};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};
use vorpal_sdk::{
    artifact::{get_env_key, step, ConfigArtifactBuilder},
    context::ConfigContext,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let go = go::build(context).await?;

    let name = "gopls";

    let step_script = formatdoc! {"
        pushd ./source/go-tools/gopls

        mkdir -p $VORPAL_OUTPUT/bin

        go build -o $VORPAL_OUTPUT/bin/gopls .

        go clean -modcache
    "};

    let step = step::shell(
        context,
        vec![go.clone()],
        vec![
            "GOCACHE=$VORPAL_WORKSPACE/go/cache".to_string(),
            "GOPATH=$VORPAL_WORKSPACE/go".to_string(),
            format!("PATH={}/bin", get_env_key(&go)),
        ],
        step_script,
    )
    .await?;

    ConfigArtifactBuilder::new(name.to_string())
        .with_source(go_tools())
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
}
