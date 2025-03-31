use crate::{
    artifact::{get_env_key, ConfigArtifactStepBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::config::v0::{
    ConfigArtifactStep, ConfigArtifactSystem,
    ConfigArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};

// TODO: implement amber step

pub fn bash(
    context: &mut ConfigContext,
    artifacts: Vec<String>,
    environments: Vec<String>,
    script: String,
    systems: Vec<ConfigArtifactSystem>,
) -> ConfigArtifactStep {
    let envs_path_bins = artifacts
        .iter()
        .map(|a| format!("{}/bin", get_env_key(a)))
        .collect::<Vec<String>>()
        .join(":");

    let envs_path_default = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin";
    let envs_path = format!("PATH={envs_path_bins}:{envs_path_default}");

    let mut envs = vec![envs_path];

    for env in environments {
        envs.push(env);
    }

    let script = formatdoc! {"
        #!/bin/bash
        set -euo pipefail

        {script}
    "};

    ConfigArtifactStepBuilder::new()
        .with_artifacts(artifacts, systems.clone())
        .with_entrypoint("bash", systems.clone())
        .with_environments(envs, systems.clone())
        .with_script(script, systems)
        .build(context)
}

pub async fn bwrap(
    context: &mut ConfigContext,
    arguments: Vec<String>,
    artifacts: Vec<String>,
    environments: Vec<String>,
    rootfs: Option<String>,
    script: String,
    systems: Vec<ConfigArtifactSystem>,
) -> Result<ConfigArtifactStep> {
    let mut bwrap_arguments = vec![
        vec!["--unshare-all".to_string()],
        vec!["--share-net".to_string()],
        vec!["--clearenv".to_string()],
        vec!["--chdir".to_string(), "$VORPAL_WORKSPACE".to_string()],
        vec!["--gid".to_string(), "1000".to_string()],
        vec!["--uid".to_string(), "1000".to_string()],
        vec!["--dev".to_string(), "/dev".to_string()],
        vec!["--proc".to_string(), "/proc".to_string()],
        vec!["--tmpfs".to_string(), "/tmp".to_string()],
        vec![
            "--bind".to_string(),
            "$VORPAL_OUTPUT".to_string(),
            "$VORPAL_OUTPUT".to_string(),
        ],
        vec![
            "--bind".to_string(),
            "$VORPAL_WORKSPACE".to_string(),
            "$VORPAL_WORKSPACE".to_string(),
        ],
        vec![
            "--setenv".to_string(),
            "VORPAL_OUTPUT".to_string(),
            "$VORPAL_OUTPUT".to_string(),
        ],
        vec![
            "--setenv".to_string(),
            "VORPAL_WORKSPACE".to_string(),
            "$VORPAL_WORKSPACE".to_string(),
        ],
    ]
    .into_iter()
    .flat_map(|x| x.into_iter())
    .collect::<Vec<String>>();

    let mut bwrap_artifacts = vec![];

    if let Some(rootfs) = rootfs {
        let rootfs_envkey = get_env_key(&rootfs);
        let rootfs_path_bin = format!("{rootfs_envkey}/bin");
        let rootfs_path_etc = format!("{rootfs_envkey}/etc");
        let rootfs_path_lib = format!("{rootfs_envkey}/lib");
        let rootfs_path_lib64 = format!("{rootfs_envkey}/lib64");
        let rootfs_path_sbin = format!("{rootfs_envkey}/sbin");
        let rootfs_path_usr = format!("{rootfs_envkey}/usr");

        let rootfs_arguments = vec![
            vec!["--ro-bind".to_string(), rootfs_path_bin, "/bin".to_string()],
            vec!["--ro-bind".to_string(), rootfs_path_etc, "/etc".to_string()],
            vec!["--ro-bind".to_string(), rootfs_path_lib, "/lib".to_string()],
            vec![
                "--ro-bind-try".to_string(),
                rootfs_path_lib64,
                "/lib64".to_string(),
            ],
            vec![
                "--ro-bind".to_string(),
                rootfs_path_sbin,
                "/sbin".to_string(),
            ],
            vec!["--ro-bind".to_string(), rootfs_path_usr, "/usr".to_string()],
            vec![
                "--ro-bind".to_string(),
                rootfs_envkey.clone(),
                rootfs_envkey.clone(),
            ],
            vec!["--setenv".to_string(), rootfs_envkey.clone(), rootfs_envkey],
        ]
        .into_iter()
        .flat_map(|x| x)
        .collect::<Vec<String>>();

        bwrap_arguments.extend(rootfs_arguments);
        bwrap_artifacts.push(rootfs);
    }

    // Setup artifact arguments

    let mut artifact_arguments = vec![];

    for artifact in artifacts.iter() {
        artifact_arguments.push(vec![
            "--ro-bind".to_string(),
            artifact.to_string(),
            artifact.to_string(),
        ]);
    }

    let bwrap_arguments_artifacts = artifact_arguments
        .into_iter()
        .flat_map(|x| x)
        .collect::<Vec<String>>();

    // Setup environment arguments

    let mut environment_arguments = vec![];

    for env in environments.iter() {
        let key = env.split("=").next().unwrap();

        environment_arguments.push(vec![
            "--setenv".to_string(),
            key.to_string(),
            env.to_string(),
        ]);
    }

    let bwrap_arguments_environments = environment_arguments
        .into_iter()
        .flat_map(|x| x)
        .collect::<Vec<String>>();

    // Setup all arguments

    let bwrap_arguments = bwrap_arguments
        .into_iter()
        .chain(bwrap_arguments_artifacts.into_iter())
        .chain(bwrap_arguments_environments.into_iter())
        .chain(arguments.into_iter().map(|x| x))
        .collect::<Vec<String>>();

    let step = ConfigArtifactStepBuilder::new()
        .with_arguments(bwrap_arguments, systems.clone())
        .with_artifacts(bwrap_artifacts, systems.clone())
        .with_entrypoint("bwrap", systems.clone())
        .with_environments(
            vec!["PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string()],
            systems.clone(),
        )
        .with_script(
            formatdoc! {"
                #!/bin/bash
                set -euo pipefail

                {script}
            "},
            systems,
        )
        .build(context);

    Ok(step)
}

pub async fn shell(
    context: &mut ConfigContext,
    artifacts: Vec<String>,
    environments: Vec<String>,
    script: String,
) -> Result<ConfigArtifactStep> {
    // Setup target

    let target = context.get_target();

    // Setup platform specific steps

    let step_bash = bash(
        context,
        artifacts.clone(),
        environments.clone(),
        script.to_string(),
        vec![Aarch64Darwin, X8664Darwin],
    );

    // let step_bwrap_rootfs = context.fetch_artifact("TODO").await?;

    let step_bwrap = bwrap(
        context,
        vec![],
        artifacts,
        environments,
        // Some(step_bwrap_rootfs),
        None,
        script,
        vec![Aarch64Linux, X8664Linux],
    )
    .await?;

    // Setup step

    let step = match target {
        Aarch64Darwin => step_bash,
        X8664Darwin => step_bash,
        Aarch64Linux => step_bwrap,
        X8664Linux => step_bwrap,
        _ => bail!("unsupported shell step system: {}", target.as_str_name()),
    };

    Ok(step)
}

pub fn docker(
    context: &mut ConfigContext,
    arguments: Vec<String>,
    artifacts: Vec<String>,
    systems: Vec<ConfigArtifactSystem>,
) -> ConfigArtifactStep {
    ConfigArtifactStepBuilder::new()
        .with_arguments(arguments, systems.clone())
        .with_artifacts(artifacts, systems.clone())
        .with_entrypoint("docker", systems.clone())
        .with_environments(
            vec!["PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string()],
            systems.clone(),
        )
        .build(context)
}
