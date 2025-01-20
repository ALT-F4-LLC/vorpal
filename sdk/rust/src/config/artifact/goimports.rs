use crate::config::{
    artifact::{add_artifact, get_artifact_envkey, go, ConfigContext},
    source::go_tools,
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let go = go::artifact(context).await?;

    let name = "goimports";

    let source = go_tools(context).await?;

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
            pushd ./source/go-tools

            mkdir -p $VORPAL_OUTPUT/bin

            go build -o $VORPAL_OUTPUT/bin/goimports ./cmd/goimports

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
