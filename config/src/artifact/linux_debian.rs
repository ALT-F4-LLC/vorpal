use crate::{
    artifact::{new_source, run_bash_step, run_docker_step, step_env_artifact},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId,
    ArtifactSystem::{Aarch64Linux, X8664Linux},
};

pub fn artifact(context: &mut ContextConfig) -> Result<ArtifactId> {
    let environments = vec![ArtifactEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
    }];

    let systems = vec![Aarch64Linux.into(), X8664Linux.into()];

    let artifact_source = new_source(
        vec![],
        None,
        vec![
            "Dockerfile".to_string(),
            "script/version_check.sh".to_string(),
        ],
        "docker".to_string(),
        ".".to_string(),
    )?;

    let artifact_source = context.add_artifact(Artifact {
        artifacts: vec![],
        name: "linux-debian-source".to_string(),
        sources: vec![artifact_source.clone()],
        steps: vec![run_bash_step(
            environments.clone(),
            "cp -prv $VORPAL_WORKSPACE/source/docker/. $VORPAL_OUTPUT/".to_string(),
        )],
        systems: systems.clone(),
    })?;

    context.add_artifact(Artifact {
        artifacts: vec![artifact_source.clone()],
        name: "linux-debian".to_string(),
        sources: vec![],
        steps: vec![
            run_docker_step(vec![
                "buildx".to_string(),
                "build".to_string(),
                "--progress=plain".to_string(),
                "--tag=altf4llc/debian:latest".to_string(),
                step_env_artifact(&artifact_source),
            ]),
            run_docker_step(vec![
                "container".to_string(),
                "create".to_string(),
                "--name".to_string(),
                artifact_source.clone().hash.to_string(),
                "altf4llc/debian:latest".to_string(),
            ]),
            run_docker_step(vec![
                "container".to_string(),
                "export".to_string(),
                "--output".to_string(),
                "$VORPAL_WORKSPACE/debian.tar".to_string(),
                artifact_source.hash.to_string(),
            ]),
            run_bash_step(
                environments.clone(),
                formatdoc! {"
                    tar -xvf $VORPAL_WORKSPACE/debian.tar -C $VORPAL_OUTPUT
                    echo \"nameserver 1.1.1.1\" > $VORPAL_OUTPUT/etc/resolv.conf
                "},
            ),
            run_docker_step(vec![
                "container".to_string(),
                "rm".to_string(),
                "--force".to_string(),
                artifact_source.hash.to_string(),
            ]),
            run_docker_step(vec![
                "image".to_string(),
                "rm".to_string(),
                "--force".to_string(),
                "altf4llc/debian:latest".to_string(),
            ]),
        ],
        systems,
    })
}
