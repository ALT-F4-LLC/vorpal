use crate::{
    artifact::{
        get_env_key, go,
        language::go::{get_goarch, get_goos},
        step, ArtifactBuilder, ArtifactSourceBuilder,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::artifact::v0::ArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let go = go::build(context).await?;

    let name = "staticcheck";

    let source_digest = "e8f40ddbc450bf26d1855916519283f7c997ffedbcb971e2a7b92892650d92b6";

    let source_version = "2025.1.1";

    let source_path =
        format!("https://github.com/dominikh/go-tools/archive/refs/tags/{source_version}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        pushd ./source/{name}/go-tools-2025.1.1

        mkdir -p $VORPAL_OUTPUT/bin

        go build -o $VORPAL_OUTPUT/bin/staticcheck cmd/staticcheck/staticcheck.go

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
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
