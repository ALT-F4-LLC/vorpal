use crate::config::{
    artifact::{
        add_artifact_source,
        steps::{bash, docker},
    },
    ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId, ArtifactSource,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let hash = "465cebbdf76af0825c160bdad35db506955c47d149972c30ae7a0629c252439f";

    let source = add_artifact_source(
        context,
        ArtifactSource {
            excludes: vec![],
            hash: Some(hash.to_string()),
            includes: vec![
                "Dockerfile".to_string(),
                "script/version_check.sh".to_string(),
            ],
            name: "docker".to_string(),
            path: ".".to_string(),
        },
    )
    .await?;

    let image_tag = format!("altf4llc/debin:{}", hash);

    context.add_artifact(Artifact {
        artifacts: vec![],
        name: "linux-debian".to_string(),
        sources: vec![source],
        steps: vec![
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
                hash.to_string(),
                image_tag.clone(),
            ]),
            docker(vec![
                "container".to_string(),
                "export".to_string(),
                "--output".to_string(),
                "$VORPAL_WORKSPACE/debian.tar".to_string(),
                hash.to_string(),
            ]),
            bash(
                vec![ArtifactEnvironment {
                    key: "PATH".to_string(),
                    value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
                }],
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
                hash.to_string(),
            ]),
            docker(vec![
                "image".to_string(),
                "rm".to_string(),
                "--force".to_string(),
                image_tag,
            ]),
        ],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    })
}
