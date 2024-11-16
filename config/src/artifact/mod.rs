use crate::{cross_platform::get_sed_cmd, ContextConfig};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId, ArtifactSource,
};

pub mod cargo;
pub mod cross_toolchain;
pub mod cross_toolchain_rootfs;
pub mod language;
pub mod protoc;
pub mod rust_std;
pub mod rustc;
pub mod zlib;

pub fn build_artifact(
    context: &mut ContextConfig,
    artifacts: Vec<ArtifactId>,
    environments: Vec<ArtifactEnvironment>,
    name: String,
    script: String,
    sources: Vec<ArtifactSource>,
    systems: Vec<i32>,
) -> Result<ArtifactId> {
    let artifact_sandbox_rootfs = cross_toolchain_rootfs::artifact(context)?;
    let artifact_sandbox = cross_toolchain::artifact(context, &artifact_sandbox_rootfs)?;

    // TODO: build artifacts from toolchain instead of using toolchain

    // Setup PATH variable

    let path = ArtifactEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/usr/sbin".to_string(),
    };

    let mut artifact_environments = vec![];

    for env in environments.clone().into_iter() {
        if env.key == path.key {
            continue;
        }

        artifact_environments.push(env);
    }

    let path_prev = environments.into_iter().find(|env| env.key == path.key);

    if let Some(prev) = path_prev {
        artifact_environments.push(ArtifactEnvironment {
            key: path.key.clone(),
            value: format!("{}:{}", prev.value, path.value),
        });
    } else {
        artifact_environments.push(path);
    }

    // Setup artifacts

    let mut artifact_artifacts = vec![];

    artifact_artifacts.push(artifact_sandbox.clone());

    for artifact in artifacts {
        artifact_artifacts.push(artifact);
    }

    let artifact = Artifact {
        environments: artifact_environments,
        name,
        artifacts: artifact_artifacts,
        sandbox: Some(artifact_sandbox.clone()),
        script: formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            {script}",
            script = script,
        },
        sources,
        systems,
    };

    context.add_artifact(artifact)
}
