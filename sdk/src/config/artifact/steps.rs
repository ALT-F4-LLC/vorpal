use crate::config::artifact::get_artifact_envkey;
use indoc::formatdoc;
use std::collections::BTreeMap;
use std::env::var;
use vorpal_schema::vorpal::artifact::v0::{ArtifactId, ArtifactStep, ArtifactStepEnvironment};

// TODO: implement cache for sources

// TODO: implement amber step

pub fn bash(environment: BTreeMap<&str, String>, script: String) -> ArtifactStep {
    let mut environment = environment.clone();

    let path_defined_default = "".to_string();
    let path_defined = environment.get("PATH").unwrap_or(&path_defined_default);

    let path_default = "/usr/bin:/usr/sbin".to_string();
    let mut path = var("PATH").unwrap_or_else(|_| path_default);

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
            #!/bin/sh
            set -euo pipefail

            {script}",
            script = script,
        }),
    }
}

pub fn bwrap(
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

    ArtifactStep {
        arguments: args,
        entrypoint: Some("bwrap".to_string()),
        environments: vec![ArtifactStepEnvironment {
            key: "PATH".to_string(),
            value: var("PATH").unwrap_or_else(|_| "/usr/bin:/usr/sbin".to_string()),
        }],
        script: Some(script),
    }
}

pub fn docker(arguments: Vec<String>) -> ArtifactStep {
    ArtifactStep {
        arguments,
        entrypoint: Some("docker".to_string()),
        environments: vec![ArtifactStepEnvironment {
            key: "PATH".to_string(),
            value: var("PATH").unwrap_or_else(|_| "/usr/bin:/usr/sbin".to_string()),
        }],
        script: None,
    }
}
