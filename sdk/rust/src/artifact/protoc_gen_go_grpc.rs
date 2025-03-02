use crate::{
    artifact::{add_artifact, get_artifact_envkey, go, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{ArtifactId, ArtifactSourceId};

pub async fn source(context: &mut ConfigContext, version: &str) -> Result<ArtifactSourceId> {
    let hash = "eba0f83ab252cffe2c6209f894c4c8238b2473a403bbdbcb985af25140aac95d";

    context
        .add_artifact_source(
            "protoc-gen-go-grpc",
            ArtifactSource {
                excludes: vec![],
                hash: Some(hash.to_string()),
                includes: vec![],
                path: format!(
                    "https://github.com/grpc/grpc-go/archive/refs/tags/v{}.tar.gz",
                    version
                ),
            },
        )
        .await
}

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let go = go::artifact(context).await?;

    let name = "protoc-gen-go-grpc";

    let version = "1.70.0";

    let source = source(context, version).await?;

    add_artifact(
        context,
        vec![go.clone()],
        BTreeMap::from([
            ("GOCACHE", "$VORPAL_WORKSPACE/go/cache".to_string()),
            ("GOPATH", "$VORPAL_WORKSPACE/go".to_string()),
            ("PATH", format!("{}/bin", get_artifact_envkey(&go))),
        ]),
        name,
        formatdoc! {"
            pushd ./source/protoc-gen-go-grpc/grpc-go-{version}

            mkdir -p $VORPAL_OUTPUT/bin

            pushd ./cmd/protoc-gen-go-grpc

            go build -o $VORPAL_OUTPUT/bin/protoc-gen-go-grpc .

            go clean -modcache
        "},
        vec![source],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
