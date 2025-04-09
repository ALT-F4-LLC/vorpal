use crate::{
    artifact::{
        get_env_key, go,
        language::go::{get_goarch, get_goos},
        step, ArtifactBuilder,
    },
    context::ConfigContext,
    source::go_tools,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::artifact::v0::ArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
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
            "CGO_ENABLED=0".to_string(),
            format!("GOARCH={}", get_goarch(context.get_target())),
            "GOCACHE=$VORPAL_WORKSPACE/go/cache".to_string(),
            format!("GOOS={}", get_goos(context.get_target())),
            "GOPATH=$VORPAL_WORKSPACE/go".to_string(),
            format!("PATH={}/bin", get_env_key(&go)),
        ],
        step_script,
    )
    .await?;

    ArtifactBuilder::new(name)
        .with_source(go_tools())
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
