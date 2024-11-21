use crate::{
    artifact::{run_bash_step, run_docker_step, step_env_artifact},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactId, ArtifactSource,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let source = context.add_artifact(Artifact {
        artifacts: vec![],
        name: "debian-slim-source".to_string(),
        sources: vec![ArtifactSource {
            excludes: vec![],
            includes: vec![
                "Dockerfile".to_string(),
                "script/version_check.sh".to_string(),
            ],
            name: "docker".to_string(),
            path: ".".to_string(),
        }],
        steps: vec![run_bash_step(
            vec![],
            "rsync -av $VORPAL_WORKSPACE/source/docker/ $VORPAL_OUTPUT".to_string(),
        )],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    })?;

    let environments = vec![];

    context.add_artifact(Artifact {
        artifacts: vec![source.clone()],
        name: "debian-slim".to_string(),
        sources: vec![ArtifactSource {
            excludes: vec![],
            includes: vec![
                "Dockerfile".to_string(),
                "script/version_check.sh".to_string(),
            ],
            name: "docker".to_string(),
            path: ".".to_string(),
        }],
        steps: vec![
            run_docker_step(
                vec![
                    "buildx".to_string(),
                    "build".to_string(),
                    "--progress=plain".to_string(),
                    "--tag=altf4llc/debian-slim:latest".to_string(),
                    step_env_artifact(&source),
                ],
                environments.clone(),
            ),
            run_docker_step(
                vec![
                    "container".to_string(),
                    "create".to_string(),
                    "--name".to_string(),
                    source.clone().hash.to_string(),
                    "altf4llc/debian-slim:latest".to_string(),
                ],
                environments.clone(),
            ),
            run_docker_step(
                vec![
                    "container".to_string(),
                    "export".to_string(),
                    "--output".to_string(),
                    "$VORPAL_WORKSPACE/debian-slim.tar".to_string(),
                    source.hash.to_string(),
                ],
                environments.clone(),
            ),
            run_bash_step(
                environments.clone(),
                formatdoc! {"
                    tar -xvf $VORPAL_WORKSPACE/debian-slim.tar -C $VORPAL_OUTPUT
                    echo \"nameserver 1.1.1.1\" > $VORPAL_OUTPUT/etc/resolv.conf
                "},
            ),
            run_docker_step(
                vec![
                    "container".to_string(),
                    "rm".to_string(),
                    "--force".to_string(),
                    source.hash.to_string(),
                ],
                environments.clone(),
            ),
            run_docker_step(
                vec![
                    "image".to_string(),
                    "rm".to_string(),
                    "--force".to_string(),
                    "altf4llc/debian-slim:latest".to_string(),
                ],
                environments.clone(),
            ),
        ],
        systems: vec![Aarch64Linux.into(), X8664Linux.into()],
    })
}
