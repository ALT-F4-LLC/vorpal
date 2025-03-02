use crate::context::ConfigContext;
use anyhow::{bail, Result};
use indoc::formatdoc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSourceId, ArtifactStep, ArtifactStepEnvironment, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub mod cargo;
pub mod clippy;
pub mod go;
pub mod goimports;
pub mod gopls;
pub mod language;
pub mod linux_debian;
pub mod linux_vorpal;
pub mod protoc;
pub mod protoc_gen_go;
pub mod protoc_gen_go_grpc;
pub mod rust_analyzer;
pub mod rust_src;
pub mod rust_std;
pub mod rustc;
pub mod rustfmt;
pub mod shell;
pub mod steps;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ArtifactSource {
    pub excludes: Vec<String>,
    pub hash: Option<String>,
    pub includes: Vec<String>,
    pub path: String,
}

#[derive(Debug, PartialEq)]
pub enum ArtifactSourceKind {
    UnknownSourceKind,
    Git,
    Http,
    Local,
}

// TODO: implement amber step

pub fn bash_step(environment: BTreeMap<&str, String>, script: String) -> ArtifactStep {
    let mut environment = environment.clone();

    let path_defined_default = "".to_string();
    let path_defined = environment.get("PATH").unwrap_or(&path_defined_default);

    let mut path = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string();

    if !path_defined.is_empty() {
        path = format!("{}:{}", path_defined, path);
    }

    environment.insert("PATH", path);

    let mut environments = vec![];

    for (key, value) in environment {
        environments.push(ArtifactStepEnvironment {
            key: key.to_string(),
            value,
        });
    }

    ArtifactStep {
        arguments: vec![],
        entrypoint: Some("bash".to_string()),
        environments,
        script: Some(formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            {script}",
            script = script,
        }),
    }
}

pub fn bwrap_step(
    arguments: Vec<String>,
    artifacts: Vec<ArtifactId>,
    environment: BTreeMap<&str, String>,
    rootfs: Option<ArtifactId>,
    script: String,
) -> ArtifactStep {
    let mut args = vec![
        "--unshare-all".to_string(),
        "--share-net".to_string(),
        "--clearenv".to_string(),
        "--chdir".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        "--gid".to_string(),
        "1000".to_string(),
        "--uid".to_string(),
        "1000".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--bind".to_string(),
        "$VORPAL_OUTPUT".to_string(),
        "$VORPAL_OUTPUT".to_string(),
        "--bind".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
        "--setenv".to_string(),
        "VORPAL_OUTPUT".to_string(),
        "$VORPAL_OUTPUT".to_string(),
        "--setenv".to_string(),
        "VORPAL_WORKSPACE".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
    ];

    if let Some(rootfs) = rootfs {
        let rootfs = get_artifact_envkey(&rootfs);

        args = [
            args,
            vec![
                // mount bin
                "--ro-bind".to_string(),
                format!("{}/bin", rootfs),
                "/bin".to_string(),
                // mount etc
                "--ro-bind".to_string(),
                format!("{}/etc", rootfs),
                "/etc".to_string(),
                // mount lib
                "--ro-bind".to_string(),
                format!("{}/lib", rootfs),
                "/lib".to_string(),
                // mount lib64 (if exists)
                "--ro-bind-try".to_string(),
                format!("{}/lib64", rootfs),
                "/lib64".to_string(),
                // mount sbin
                "--ro-bind".to_string(),
                format!("{}/sbin", rootfs),
                "/sbin".to_string(),
                // mount usr
                "--ro-bind".to_string(),
                format!("{}/usr", rootfs),
                "/usr".to_string(),
            ],
        ]
        .concat();
    }

    for artifact in artifacts {
        // add read-only mounts
        args.push("--ro-bind".to_string());
        args.push(get_artifact_envkey(&artifact));
        args.push(get_artifact_envkey(&artifact));

        // add environment variables
        args.push("--setenv".to_string());
        args.push(get_artifact_envkey(&artifact).replace("$", ""));
        args.push(get_artifact_envkey(&artifact));
    }

    for (key, value) in environment.clone() {
        args.push("--setenv".to_string());
        args.push(key.to_string());
        args.push(value.to_string());
    }

    for arg in arguments {
        args.push(arg);
    }

    let path = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string();

    ArtifactStep {
        arguments: args,
        entrypoint: Some("bwrap".to_string()),
        environments: vec![ArtifactStepEnvironment {
            key: "PATH".to_string(),
            value: path,
        }],
        script: Some(script),
    }
}

pub fn docker_step(arguments: Vec<String>) -> ArtifactStep {
    let path = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string();

    ArtifactStep {
        arguments,
        entrypoint: Some("docker".to_string()),
        environments: vec![ArtifactStepEnvironment {
            key: "PATH".to_string(),
            value: path,
        }],
        script: None,
    }
}

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
    sources: Vec<ArtifactSourceId>,
    systems: Vec<&str>,
) -> Result<ArtifactId> {
    // Setup target

    let target = context.get_target();

    // Setup artifacts

    let mut artifacts = artifacts.clone();

    if target == Aarch64Linux || target == X8664Linux {
        let linux_debian = linux_debian::artifact(context).await?;
        let linux_vorpal = linux_vorpal::artifact(context, &linux_debian).await?;

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

        steps.push(bwrap_step(
            vec![],
            artifacts.clone(),
            env.clone(),
            Some(linux_vorpal.clone()),
            script.to_string(),
        ));
    }

    if target == Aarch64Macos || target == X8664Macos {
        steps.push(bash_step(env.clone(), script.to_string()));
    }

    // Add artifact to context

    context
        .add_artifact(name, artifacts, sources, steps, systems)
        .await
}
