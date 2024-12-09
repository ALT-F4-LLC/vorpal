use crate::config::{
    artifact::{
        steps::{bash, bwrap},
        toolchain::linux::{debian, vorpal},
    },
    ConfigContext,
};
use anyhow::{bail, Result};
use std::path::Path;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId, ArtifactSource, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};
use vorpal_store::paths::get_file_paths;

pub mod language;
pub mod steps;
pub mod toolchain;

pub fn get_artifact_envkey(artifact: &ArtifactId) -> String {
    format!(
        "$VORPAL_ARTIFACT_{}",
        artifact.name.to_lowercase().replace("-", "_")
    )
    .to_string()
}

pub async fn add_artifact_source(
    context: &mut ConfigContext,
    source: ArtifactSource,
) -> Result<ArtifactSource> {
    // TODO: add support for 'remote' sources

    let source_path = Path::new(&source.path).to_path_buf();

    if !source_path.exists() {
        bail!(
            "Artifact `source.{}.path` not found: {:?}",
            source.name,
            source.path
        );
    }

    let source_files = get_file_paths(
        &source_path,
        source.excludes.clone(),
        source.includes.clone(),
    )?;

    let source_hash = match context.get_source_hash(
        source_files.clone(),
        source.name.clone(),
        source_path.clone(),
    ) {
        Some(hash) => hash.clone(),
        None => {
            context
                .add_source_hash(
                    source_files.clone(),
                    source.name.clone(),
                    source_path.clone(),
                )
                .await?
        }
    };

    if let Some(hash) = source.hash.clone() {
        if hash != source_hash {
            bail!(
                "Artifact `source.{}.hash` mismatch: {} != {}",
                source.name,
                hash,
                source_hash
            );
        }
    }

    Ok(ArtifactSource {
        excludes: source.excludes,
        hash: Some(source_hash),
        includes: source.includes,
        name: source.name,
        path: source.path,
    })
}

pub fn add_artifact_systems(systems: Vec<&str>) -> Result<Vec<ArtifactSystem>> {
    let mut build_systems = vec![];

    for system in systems {
        match system {
            "aarch64-linux" => build_systems.push(Aarch64Linux),
            "aarch64-macos" => build_systems.push(Aarch64Macos),
            "x86_64-linux" => build_systems.push(X8664Linux),
            "x86_64-macos" => build_systems.push(X8664Macos),
            _ => bail!("Unsupported system: {}", system),
        }
    }

    Ok(build_systems)
}

pub async fn add_artifact(
    context: &mut ConfigContext,
    artifacts: Vec<ArtifactId>,
    environments: Vec<ArtifactEnvironment>,
    name: &str,
    script: String,
    sources: Vec<ArtifactSource>,
    systems: Vec<&str>,
) -> Result<ArtifactId> {
    // Setup artifacts

    let mut build_artifacts = vec![];

    for artifact in artifacts {
        build_artifacts.push(artifact);
    }

    // Setup environments

    let build_target = context.get_target();

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

    // Setup sources

    let mut build_sources = vec![];

    for source in sources.clone().into_iter() {
        let source = add_artifact_source(context, source).await?;

        build_sources.push(source);
    }

    // Setup steps

    let mut build_steps = vec![];

    if build_target == Aarch64Linux || build_target == X8664Linux {
        let linux_debian = debian::artifact(context).await?;
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

    // Setup systems

    let systems = add_artifact_systems(systems)?;
    let systems = systems.iter().map(|s| (*s).into()).collect::<Vec<i32>>();

    // Add artifact to context

    context.add_artifact(Artifact {
        artifacts: build_artifacts.clone(),
        name: name.to_string(),
        sources: build_sources,
        steps: build_steps,
        systems,
    })
}
