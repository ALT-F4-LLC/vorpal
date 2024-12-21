use crate::config::{
    artifact::steps::{bash, docker},
    ArtifactSource, ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let source_hash = "465cebbdf76af0825c160bdad35db506955c47d149972c30ae7a0629c252439f";

    let image_tag = format!("altf4llc/debin:{}", source_hash);

    context
        .add_artifact(
            "linux-debian",
            vec![],
            BTreeMap::from([(
                "docker",
                ArtifactSource {
                    excludes: vec![],
                    hash: Some(source_hash.to_string()),
                    includes: vec![
                        "Dockerfile".to_string(),
                        "script/version_check.sh".to_string(),
                    ],
                    path: ".".to_string(),
                },
            )]),
            vec![
                docker(vec![
                    "buildx".to_string(),
                    "build".to_string(),
                    "--progress=plain".to_string(),
                    format!("--tag={}", image_tag),
                    "$VORPAL_WORKSPACE/source/docker".to_string(),
                ]),
                docker(vec![
                    "container".to_string(),
                    "create".to_string(),
                    "--name".to_string(),
                    source_hash.to_string(),
                    image_tag.clone(),
                ]),
                docker(vec![
                    "container".to_string(),
                    "export".to_string(),
                    "--output".to_string(),
                    "$VORPAL_WORKSPACE/debian.tar".to_string(),
                    source_hash.to_string(),
                ]),
                bash(
                    BTreeMap::new(),
                    formatdoc! {"
                        ## extract files
                        tar -xvf $VORPAL_WORKSPACE/debian.tar -C $VORPAL_OUTPUT

                        ## patch files
                        echo \"nameserver 1.1.1.1\" > $VORPAL_OUTPUT/etc/resolv.conf
                    "},
                ),
                docker(vec![
                    "container".to_string(),
                    "rm".to_string(),
                    "--force".to_string(),
                    source_hash.to_string(),
                ]),
                docker(vec![
                    "image".to_string(),
                    "rm".to_string(),
                    "--force".to_string(),
                    image_tag,
                ]),
            ],
            vec!["aarch64-linux", "x86_64-linux"],
        )
        .await
}
