use crate::{
    artifact::{
        get_env_key, go,
        language::go::{get_goarch, get_goos},
        protoc, step, ArtifactBuilder, ArtifactSourceBuilder,
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
    let protoc = protoc::build(context).await?;

    let name = "grpcurl";

    let source_version = "1.9.3";

    let source_path =
        format!("https://github.com/fullstorydev/grpcurl/releases/tag/v{source_version}",);

    let source_digest = "3db5cef04f38e71c4007ed96cc827209ae5a1b6613c710cd656a252fafcde86c";

    let source = ArtifactSourceBuilder::new(name, &source_path)
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        mkdir -p $VORPAL_OUTPUT/bin

        pushd ./source/{name}/grpcurl-{source_version}

        go build -o $VORPAL_OUTPUT/bin/grpcurl ./cmd/grpcurl

        chmod +x $VORPAL_OUTPUT/bin/grpcurl

        go clean -modcache
    "};

    let step = step::shell(
        context,
        vec![go.clone(), protoc.clone()],
        vec![
            "CGO_ENABLED=0".to_string(),
            "GOCACHE=$VORPAL_WORKSPACE/go/cache".to_string(),
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
