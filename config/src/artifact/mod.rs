use crate::ContextConfig;
use anyhow::{bail, Result};
use indoc::formatdoc;
use std::path::Path;
use vorpal_schema::vorpal::artifact::v0::{
    Artifact, ArtifactEnvironment, ArtifactId, ArtifactSource, ArtifactStep,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};
use vorpal_store::{hashes::hash_files, paths::get_file_paths};

pub mod cargo;
pub mod language;
pub mod linux_debian;
pub mod linux_vorpal;
pub mod protoc;
pub mod rust_std;
pub mod rustc;

// TODO: implement cache for sources

pub fn new_artifact_source(
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

pub fn step_env_artifact(artifact: &ArtifactId) -> String {
    let artifact_key = artifact.name.to_lowercase().replace("-", "_");
    format!("$VORPAL_ARTIFACT_{}", artifact_key).to_string()
}

pub fn run_bash_step(environments: Vec<ArtifactEnvironment>, script: String) -> ArtifactStep {
    ArtifactStep {
        arguments: vec![],
        entrypoint: None,
        environments,
        script: Some(formatdoc! {"
            #!/bin/bash
            set -euo pipefail

            {script}",
            script = script,
        }),
    }
}

pub fn run_bwrap_step(
    arguments: Vec<String>,
    artifacts: Vec<ArtifactId>,
    environments: Vec<ArtifactEnvironment>,
    rootfs: Option<String>,
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
        args.push(step_env_artifact(&artifact));
        args.push(step_env_artifact(&artifact));

        // add environment variables
        args.push("--setenv".to_string());
        args.push(step_env_artifact(&artifact).replace("$", ""));
        args.push(step_env_artifact(&artifact));
    }

    for env in environments.clone() {
        args.push("--setenv".to_string());
        args.push(env.key.clone());
        args.push(env.value.clone());
    }

    for arg in arguments {
        args.push(arg);
    }

    // TODO: use amber instead of bash as a proof of concept

    run_bash_step(
        environments,
        formatdoc! {"
            cat > $VORPAL_WORKSPACE/bwrap.sh << \"EOS\"
            #!/bin/bash
            set -euo pipefail

            {script}
            EOS

            chmod +x $VORPAL_WORKSPACE/bwrap.sh

            {entrypoint} {arguments} $VORPAL_WORKSPACE/bwrap.sh",
            entrypoint = "/usr/bin/bwrap",
            arguments = args.join(" "),
        },
    )
}

pub fn run_docker_step(arguments: Vec<String>) -> ArtifactStep {
    run_bash_step(
        vec![],
        format!("{} {}", "/usr/bin/docker", arguments.join(" ")),
    )
}

pub fn build_artifact(
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
        let linux_debian = linux_debian::artifact(context)?;
        let linux_vorpal = linux_vorpal::artifact(context, &linux_debian)?;

        build_artifacts.push(linux_vorpal.clone());

        build_steps.push(run_bwrap_step(
            vec![],
            build_artifacts.clone(),
            build_environments.clone(),
            Some(step_env_artifact(&linux_vorpal)),
            script.clone(),
        ));
    }

    if build_target == Aarch64Macos || build_target == X8664Macos {
        build_steps.push(run_bash_step(build_environments.clone(), script));
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
