use crate::config::{
    artifact::{
        steps::{bash, bwrap},
        toolchain::linux::{debian, vorpal},
    },
    ArtifactSource, ConfigContext,
};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactStepEnvironment, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub mod language;
pub mod shell;
pub mod steps;
pub mod toolchain;

pub fn get_artifact_envkey(artifact: &ArtifactId) -> String {
    format!(
        "$VORPAL_ARTIFACT_{}",
        artifact.name.to_lowercase().replace("-", "_")
    )
    .to_string()
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

// cross-platform sandboxed artifact

pub async fn add_artifact(
    context: &mut ConfigContext,
    artifacts: Vec<ArtifactId>,
    environment: BTreeMap<&str, String>,
    name: &str,
    script: String,
    source: BTreeMap<&str, ArtifactSource>,
    systems: Vec<&str>,
) -> Result<ArtifactId> {
    // Setup target

    let target = context.get_target();

    // Setup artifacts

    let mut artifacts = artifacts.clone();

    if target == Aarch64Linux || target == X8664Linux {
        let linux_debian = debian::artifact(context).await?;
        let linux_vorpal = vorpal::artifact(context, &linux_debian).await?;

        artifacts.push(linux_vorpal.clone());
    }

    // Setup environments

    let mut env = BTreeMap::new();

    if target == Aarch64Linux || target == X8664Linux {
        let env_path = ArtifactStepEnvironment {
            key: "PATH".to_string(),
            value: "/usr/bin:/usr/sbin".to_string(),
        };

        let env_ssl_cert_file = ArtifactStepEnvironment {
            key: "SSL_CERT_FILE".to_string(),
            value: "/etc/ssl/certs/ca-certificates.crt".to_string(),
        };

        env.insert("PATH", env_path.value);
        env.insert("SSL_CERT_FILE", env_ssl_cert_file.value);
    }

    if target == Aarch64Macos || target == X8664Macos {
        let env_path = ArtifactStepEnvironment {
            key: "PATH".to_string(),
            value: "/usr/local/bin:/usr/bin:/usr/sbin:/bin".to_string(),
        };

        env.insert("PATH", env_path.value);
    }

    // Add environment path if defined

    if let Some(new_path) = environment.get("PATH") {
        if !new_path.is_empty() {
            if let Some(old_path) = env.get("PATH") {
                env.insert("PATH", format!("{}:{}", new_path, old_path));
            }
        }
    }

    // Add environment variables

    for (key, value) in environment.clone() {
        if key == "PATH" {
            continue;
        }

        env.insert(key, value);
    }

    // Setup steps

    let mut steps = vec![];

    if target == Aarch64Linux || target == X8664Linux {
        let linux_vorpal = artifacts
            .iter()
            .find(|a| a.name == "linux-vorpal")
            .expect("linux-vorpal artifact not found");

        steps.push(bwrap(
            vec![],
            artifacts.clone(),
            env.clone(),
            Some(linux_vorpal.clone()),
            script.to_string(),
        ));
    }

    if target == Aarch64Macos || target == X8664Macos {
        steps.push(bash(env.clone(), script.to_string()));
    }

    // Add artifact to context

    context
        .add_artifact(name, artifacts, source, steps, systems)
        .await
}
