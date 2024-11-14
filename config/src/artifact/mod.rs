use crate::{cross_platform::get_sed_cmd, ContextConfig};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{Artifact, ArtifactEnvironment, ArtifactId};

pub mod cargo;
pub mod cross_toolchain;
pub mod cross_toolchain_rootfs;
pub mod language;
pub mod protoc;
pub mod rust_std;
pub mod rustc;
pub mod zlib;

pub fn build_artifact(context: &mut ContextConfig, artifact: Artifact) -> Result<ArtifactId> {
    let cross_toolchain_rootfs = cross_toolchain_rootfs::artifact(context)?;

    let cross_toolchain = cross_toolchain::artifact(context, &cross_toolchain_rootfs)?;
    let cross_toolchain_envkey = cross_toolchain.name.to_lowercase().replace("-", "_");

    // TODO: build artifacts from toolchain instead of using toolchain

    // Setup PATH variable

    let path = ArtifactEnvironment {
        key: "PATH".to_string(),
        value: "/usr/bin:/usr/sbin".to_string(),
    };

    let mut environments = vec![];

    for env in artifact.environments.clone().into_iter() {
        if env.key == path.key {
            continue;
        }

        environments.push(env);
    }

    let path_prev = artifact
        .environments
        .into_iter()
        .find(|env| env.key == path.key);

    if let Some(prev) = path_prev {
        environments.push(ArtifactEnvironment {
            key: path.key.clone(),
            value: format!("{}:{}", prev.value, path.value),
        });
    } else {
        environments.push(path);
    }

    // Setup artifacts

    let mut artifacts = vec![];

    artifacts.push(cross_toolchain.clone());

    for artifact in artifact.artifacts {
        artifacts.push(artifact);
    }

    let artifact = Artifact {
        environments,
        name: artifact.name,
        artifacts,
        sandbox: artifact.sandbox,
        script: formatdoc! {"
            #!${cross_toolchain}/bin/bash
            set -euo pipefail

            {script}",
            cross_toolchain = cross_toolchain_envkey,
            script = artifact.script,
        },
        sources: artifact.sources,
        systems: artifact.systems,
    };

    context.add_artifact(artifact)
}
