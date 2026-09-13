use crate::{
    api,
    api::artifact::ArtifactSystem::{self, Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{self, linux_vorpal::LinuxVorpal},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Builds a plain `bash` step that runs `script` with the given artifacts on
/// `PATH` and the given extra environment variables set.
#[must_use]
pub fn bash(
    artifacts: &[String],
    environments: &[String],
    secrets: &[api::artifact::ArtifactStepSecret],
    script: &str,
) -> api::artifact::ArtifactStep {
    let mut step_environments: Vec<String> = environments
        .iter()
        .filter(|environment| !environment.starts_with("PATH="))
        .cloned()
        .collect();

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
        .with_artifacts(artifacts.to_vec())
        .with_environments(step_environments)
        .with_secrets(secrets.to_vec())
        .with_script(step_script)
        .build()
}

/// Builds a `bwrap` (bubblewrap) sandboxed step that runs `script` inside a
/// minimal, isolated root filesystem assembled from `rootfs` and `artifacts`.
///
/// # Errors
///
/// Currently infallible; the `Result` is retained so callers can treat
/// `bwrap` and [`shell`] uniformly.
#[expect(
    clippy::unused_async,
    reason = "public SDK API called as `.await` from every artifact module; removing async is a breaking change"
)]
#[expect(
    clippy::too_many_lines,
    reason = "assembles one flat bwrap argument list; splitting it would scatter one linear construction across helpers without adding clarity"
)]
pub async fn bwrap(
    arguments: &[&str],
    artifacts: &[String],
    environments: &[String],
    rootfs: Option<&str>,
    secrets: &[api::artifact::ArtifactStepSecret],
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

    let mut step_artifacts: Vec<String> = vec![];

    if let Some(rootfs) = rootfs {
        let rootfs_env = artifact::get_env_key(rootfs);

        let rootfs_args = vec![
            "--ro-bind".to_string(),
            format!("{rootfs_env}/bin"),
            "/bin".to_string(),
            "--ro-bind".to_string(),
            format!("{rootfs_env}/etc"),
            "/etc".to_string(),
            "--ro-bind".to_string(),
            format!("{rootfs_env}/lib"),
            "/lib".to_string(),
            "--ro-bind-try".to_string(),
            format!("{rootfs_env}/lib64"),
            "/lib64".to_string(),
            "--ro-bind".to_string(),
            format!("{rootfs_env}/sbin"),
            "/sbin".to_string(),
            "--ro-bind".to_string(),
            format!("{rootfs_env}/usr"),
            "/usr".to_string(),
        ];

        step_arguments.extend(rootfs_args);
        step_artifacts.push(rootfs.to_string());
    }

    // Setup artifact arguments

    step_artifacts.extend_from_slice(artifacts);

    for artifact in &step_artifacts {
        step_arguments.push("--ro-bind".to_string());
        step_arguments.push(artifact::get_env_key(artifact));
        step_arguments.push(artifact::get_env_key(artifact));
        step_arguments.push("--setenv".to_string());
        step_arguments.push(artifact::get_env_key(artifact).replace('$', ""));
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

    for env in environments {
        let Some((key, value)) = env.split_once('=') else {
            continue;
        };

        if key == "PATH" {
            continue;
        }

        step_arguments.push("--setenv".to_string());
        step_arguments.push(key.to_string());
        step_arguments.push(value.to_string());
    }

    // Setup arguments

    for argument in arguments {
        step_arguments.push((*argument).to_string());
    }

    // Setup script

    let step_script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        {script}
    "};

    // Setup step

    let step = artifact::ArtifactStep::new("bwrap")
        .with_arguments(
            step_arguments
                .iter()
                .map(std::string::String::as_str)
                .collect(),
        )
        .with_artifacts(step_artifacts)
        .with_environments(vec![
            "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string()
        ])
        .with_secrets(secrets.to_vec())
        .with_script(step_script)
        .build();

    Ok(step)
}

/// Builds a step appropriate for the context's target system: a plain `bash`
/// step on Darwin, or a `bwrap`-sandboxed step backed by `linux-vorpal` on
/// Linux.
///
/// # Errors
///
/// Returns an error if the context's target system is [`ArtifactSystem::UnknownSystem`],
/// or if building the `linux-vorpal` root filesystem for a Linux target fails.
pub async fn shell(
    context: &mut ConfigContext,
    artifacts: &[String],
    environments: &[String],
    script: String,
    secrets: &[api::artifact::ArtifactStepSecret],
) -> Result<api::artifact::ArtifactStep> {
    // Setup target

    let step_system = context.get_system();

    // Setup step

    let step = match step_system {
        Aarch64Darwin | X8664Darwin => bash(artifacts, environments, secrets, &script),

        Aarch64Linux | X8664Linux => {
            let linux_vorpal = LinuxVorpal::new().build(context).await?;

            bwrap(
                &[],
                artifacts,
                environments,
                Some(linux_vorpal.as_str()),
                secrets,
                script,
            )
            .await?
        }

        ArtifactSystem::UnknownSystem => {
            bail!("unsupported system: {}", step_system.as_str_name())
        }
    };

    Ok(step)
}

// TODO: Add support for secrets with docker step

/// Builds a `docker` step that runs the given `docker` CLI arguments against
/// the given artifacts.
#[must_use]
pub fn docker(arguments: &[&str], artifacts: &[String]) -> api::artifact::ArtifactStep {
    artifact::ArtifactStep::new("docker")
        .with_arguments(arguments.to_vec())
        .with_artifacts(artifacts.to_vec())
        .with_environments(vec![
            "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string()
        ])
        .build()
}
