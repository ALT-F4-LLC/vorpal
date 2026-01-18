use crate::{
    api,
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{self, linux_vorpal::LinuxVorpal},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub fn bash(
    artifacts: Vec<String>,
    environments: Vec<String>,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    script: String,
) -> api::artifact::ArtifactStep {
    let mut step_environments = vec![];

    for environment in environments.iter() {
        if environment.starts_with("PATH=") {
            continue;
        }

        step_environments.push(environment.to_string());
    }

    let step_path_bins = artifacts
        .iter()
        .map(|a| format!("{}/bin", artifact::get_env_key(a)))
        .collect::<Vec<String>>()
        .join(":");

    let step_path_default = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin";

    let mut step_path = format!("{step_path_bins}:{step_path_default}");

    if let Some(path) = environments.iter().find(|x| x.starts_with("PATH=")) {
        if let Some(path_value) = path.split('=').nth(1) {
            step_path = format!("{path_value}:{step_path}");
        }
    }

    step_environments.push("HOME=$VORPAL_WORKSPACE".to_string());
    step_environments.push(format!("PATH={step_path}"));

    let step_script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        {script}
    "};

    artifact::ArtifactStep::new("bash")
        .with_artifacts(artifacts)
        .with_environments(step_environments)
        .with_secrets(secrets)
        .with_script(step_script)
        .build()
}

pub async fn bwrap(
    arguments: Vec<&str>,
    artifacts: Vec<String>,
    environments: Vec<String>,
    rootfs: Option<String>,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    script: String,
) -> Result<api::artifact::ArtifactStep> {
    // Setup arguments

    let mut step_arguments = vec![
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
        "--setenv".to_string(),
        "HOME".to_string(),
        "$VORPAL_WORKSPACE".to_string(),
    ];

    // Setup artifacts

    let mut step_artifacts = vec![];

    if let Some(rootfs) = rootfs {
        let rootfs_env = artifact::get_env_key(&rootfs);
        let rootfs_bin = format!("{rootfs_env}/bin");
        let rootfs_etc = format!("{rootfs_env}/etc");
        let rootfs_lib = format!("{rootfs_env}/lib");
        let rootfs_lib64 = format!("{rootfs_env}/lib64");
        let rootfs_sbin = format!("{rootfs_env}/sbin");
        let rootfs_usr = format!("{rootfs_env}/usr");

        let rootfs_args = vec![
            "--ro-bind".to_string(),
            rootfs_bin,
            "/bin".to_string(),
            "--ro-bind".to_string(),
            rootfs_etc,
            "/etc".to_string(),
            "--ro-bind".to_string(),
            rootfs_lib,
            "/lib".to_string(),
            "--ro-bind-try".to_string(),
            rootfs_lib64,
            "/lib64".to_string(),
            "--ro-bind".to_string(),
            rootfs_sbin,
            "/sbin".to_string(),
            "--ro-bind".to_string(),
            rootfs_usr,
            "/usr".to_string(),
        ];

        step_arguments.extend(rootfs_args);
        step_artifacts.push(rootfs);
    }

    // Setup artifact arguments

    for artifact in artifacts.into_iter() {
        step_artifacts.push(artifact);
    }

    for artifact in step_artifacts.iter() {
        step_arguments.push("--ro-bind".to_string());
        step_arguments.push(artifact::get_env_key(artifact));
        step_arguments.push(artifact::get_env_key(artifact));
        step_arguments.push("--setenv".to_string());
        step_arguments.push(artifact::get_env_key(artifact).replace("$", ""));
        step_arguments.push(artifact::get_env_key(artifact));
    }

    // Setup environment arguments

    let step_path_bins = step_artifacts
        .iter()
        .map(|a| format!("{}/bin", artifact::get_env_key(a)))
        .collect::<Vec<String>>()
        .join(":");

    let mut step_path = format!("{step_path_bins}:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin");

    if let Some(path) = environments.iter().find(|x| x.starts_with("PATH=")) {
        if let Some(path_value) = path.split('=').nth(1) {
            step_path = format!("{path_value}:{step_path}");
        }
    }

    step_arguments.push("--setenv".to_string());
    step_arguments.push("PATH".to_string());
    step_arguments.push(step_path);

    for env in environments.iter() {
        let key = env.split("=").next().unwrap();
        let value = env.split("=").last().unwrap();

        if key.starts_with("PATH") {
            continue;
        }

        step_arguments.push("--setenv".to_string());
        step_arguments.push(key.to_string());
        step_arguments.push(value.to_string());
    }

    // Setup arguments

    for argument in arguments.into_iter() {
        step_arguments.push(argument.to_string());
    }

    // Setup script

    let step_script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        {script}
    "};

    // Setup step

    let step = artifact::ArtifactStep::new("bwrap")
        .with_arguments(step_arguments.iter().map(|x| x.as_str()).collect())
        .with_artifacts(step_artifacts)
        .with_environments(vec![
            "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string()
        ])
        .with_secrets(secrets)
        .with_script(step_script)
        .build();

    Ok(step)
}

pub async fn shell(
    context: &mut ConfigContext,
    artifacts: Vec<String>,
    environments: Vec<String>,
    script: String,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
) -> Result<api::artifact::ArtifactStep> {
    // Setup target

    let step_system = context.get_system();

    // Setup step

    let step = match step_system {
        Aarch64Darwin | X8664Darwin => {
            bash(artifacts, environments.clone(), secrets, script.to_string())
        }

        Aarch64Linux | X8664Linux => {
            let linux_vorpal = LinuxVorpal::new().build(context).await?;

            bwrap(
                vec![],
                artifacts,
                environments,
                Some(linux_vorpal),
                secrets,
                script,
            )
            .await?
        }

        _ => bail!("unsupported system: {}", step_system.as_str_name()),
    };

    Ok(step)
}

// TODO: Add support for secrets with docker step

pub fn docker(arguments: Vec<&str>, artifacts: Vec<String>) -> api::artifact::ArtifactStep {
    artifact::ArtifactStep::new("docker")
        .with_arguments(arguments)
        .with_artifacts(artifacts)
        .with_environments(vec![
            "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string()
        ])
        .build()
}
