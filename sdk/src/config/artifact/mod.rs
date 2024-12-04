use crate::config::{
    artifact::{
        steps::{bash, bwrap},
        toolchain::linux::{debian, vorpal},
    },
    ContextConfig,
};
use anyhow::{bail, Result};
use std::path::Path;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId, ArtifactSource, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};
use vorpal_store::{hashes::hash_files, paths::get_file_paths};

pub mod language;
pub mod steps;
pub mod toolchain;

pub fn get_artifact_envkey(artifact: &ArtifactId) -> String {
    let artifact_key = artifact.name.to_lowercase().replace("-", "_");
    format!("$VORPAL_ARTIFACT_{}", artifact_key).to_string()
}

pub fn add_systems(systems: Vec<&str>) -> Result<Vec<ArtifactSystem>> {
    let mut build_systems = vec![];

    for system in systems {
        match system {
            "aarch64-linux" => build_systems.push(Aarch64Linux.into()),
            "aarch64-macos" => build_systems.push(Aarch64Macos.into()),
            "x86_64-linux" => build_systems.push(X8664Linux.into()),
            "x86_64-macos" => build_systems.push(X8664Macos.into()),
            _ => bail!("Unsupported system: {}", system),
        }
    }

    Ok(build_systems)
}

pub fn add_artifact_source(
    excludes: Vec<String>,
    hash: Option<String>,
    includes: Vec<String>,
    name: String,
    path: String,
) -> Result<ArtifactSource> {
    // TODO: add support for downloading sources

    let source_path = Path::new(&path).to_path_buf();

    if !source_path.exists() {
        bail!("Artifact `source.{}.path` not found: {:?}", name, path);
    }

    let source_files = get_file_paths(&source_path, excludes.clone(), includes.clone())?;

    let source_hash = hash_files(source_files)?;

    if let Some(hash) = hash.clone() {
        if hash != source_hash {
            bail!(
                "Artifact `source.{}.hash` mismatch: {} != {}",
                name,
                hash,
                source_hash
            );
        }
    }

    Ok(ArtifactSource {
        excludes,
        hash: Some(source_hash),
        includes,
        name,
        path,
    })
}

pub fn add_artifact(
    context: &mut ContextConfig,
    artifacts: Vec<ArtifactId>,
    environments: Vec<ArtifactEnvironment>,
    name: &str,
    script: String,
    sources: Vec<ArtifactSource>,
    systems: Vec<i32>,
) -> Result<ArtifactId> {
    let build_target = context.get_target();

    // Setup environments

    let mut build_environments = vec![];

    if build_target == Aarch64Linux || build_target == X8664Linux {
        let path = ArtifactEnvironment {
            key: "PATH".to_string(),
            value: "/usr/bin:/usr/sbin".to_string(),
        };

        let ssl_cert_file = ArtifactEnvironment {
            key: "SSL_CERT_FILE".to_string(),
            value: "/etc/ssl/certs/ca-certificates.crt".to_string(),
        };

        let path_prev = environments
            .clone()
            .into_iter()
            .find(|env| env.key == "PATH");

        if let Some(prev) = path_prev {
            build_environments.push(ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!("{}:{}", prev.value, path.value),
            });
        } else {
            build_environments.push(path.clone());
        }

        build_environments.push(ssl_cert_file.clone());
    }

    if build_target == Aarch64Macos || build_target == X8664Macos {
        let path = ArtifactEnvironment {
            key: "PATH".to_string(),
            value: "/usr/local/bin:/usr/bin:/usr/sbin:/bin".to_string(),
        };

        let path_prev = environments
            .clone()
            .into_iter()
            .find(|env| env.key == "PATH");

        if let Some(prev) = path_prev {
            build_environments.push(ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!("{}:{}", prev.value, path.value),
            });
        } else {
            build_environments.push(path.clone());
        }
    }

    for env in environments.clone().into_iter() {
        if env.key == "PATH" {
            continue;
        }

        build_environments.push(env);
    }

    let mut build_artifacts = vec![];
    let mut build_steps = vec![];

    // Setup artifacts

    for artifact in artifacts {
        build_artifacts.push(artifact);
    }

    // Setup steps

    if build_target == Aarch64Linux || build_target == X8664Linux {
        let linux_debian = debian::artifact(context)?;
        let linux_vorpal = vorpal::artifact(context, &linux_debian)?;

        build_artifacts.push(linux_vorpal.clone());

        build_steps.push(bwrap(
            vec![],
            build_artifacts.clone(),
            build_environments.clone(),
            Some(get_artifact_envkey(&linux_vorpal)),
            script.clone(),
        ));
    }

    if build_target == Aarch64Macos || build_target == X8664Macos {
        build_steps.push(bash(build_environments.clone(), script));
    }

    // Setup artifacts

    context.add_artifact(Artifact {
        artifacts: build_artifacts.clone(),
        name: name.to_string(),
        sources,
        steps: build_steps,
        systems,
    })
}
