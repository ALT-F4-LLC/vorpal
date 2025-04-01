use crate::artifact::go;
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::config::v0::ConfigArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};
use vorpal_sdk::{
    artifact::{get_env_key, step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder},
    context::ConfigContext,
};

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let go = go::build(context).await?;

    let name = "protoc-gen-go-grpc";

    let source_hash = "eba0f83ab252cffe2c6209f894c4c8238b2473a403bbdbcb985af25140aac95d";
    let source_version = "1.70.0";
    let source_path =
        format!("https://github.com/grpc/grpc-go/archive/refs/tags/v{source_version}.tar.gz");

    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path)
        .with_hash(source_hash.to_string())
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
            "GOCACHE=$VORPAL_WORKSPACE/go/cache".to_string(),
            "GOPATH=$VORPAL_WORKSPACE/go".to_string(),
            format!("PATH={}/bin", get_env_key(&go)),
        ],
        step_script,
    )
    .await?;

    ConfigArtifactBuilder::new(name.to_string())
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
}
