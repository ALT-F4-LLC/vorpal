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

    let name = "protoc-gen-go-grpc";

    let source_digest = "eba0f83ab252cffe2c6209f894c4c8238b2473a403bbdbcb985af25140aac95d";
    let source_version = "1.70.0";
    let source_path =
        format!("https://github.com/grpc/grpc-go/archive/refs/tags/v{source_version}.tar.gz");

    let source = ArtifactSourceBuilder::new(name, source_path.as_str())
        .with_digest(source_digest)
        .build();

    let step_script = formatdoc! {"
        pushd ./source/protoc-gen-go-grpc/grpc-go-{source_version}

        mkdir -p $VORPAL_OUTPUT/bin

        pushd ./cmd/protoc-gen-go-grpc

        go build -o $VORPAL_OUTPUT/bin/protoc-gen-go-grpc .

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
