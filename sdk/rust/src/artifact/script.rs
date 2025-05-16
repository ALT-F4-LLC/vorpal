use crate::{
    api::artifact::ArtifactSystem,
    artifact::{get_env_key, step, ArtifactBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

pub async fn devshell<'a>(
    context: &mut ConfigContext,
    artifacts: Vec<String>,
    environments: Vec<String>,
    name: &'a str,
    systems: Vec<ArtifactSystem>,
) -> Result<String> {
    let mut envs_backup = vec![
        "export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"".to_string(),
        "export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"".to_string(),
        "export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"".to_string(),
    ];

    let mut envs_export = vec![
        format!("export PS1=\"({}) $PS1\"", name),
        "export VORPAL_SHELL=\"1\"".to_string(),
    ];

    let mut envs_restore = vec![
        "export PATH=\"$VORPAL_SHELL_BACKUP_PATH\"".to_string(),
        "export PS1=\"$VORPAL_SHELL_BACKUP_PS1\"".to_string(),
        "export VORPAL_SHELL=\"$VORPAL_SHELL_BACKUP_VORPAL_SHELL\"".to_string(),
    ];

    let mut envs_unset = vec![
        "unset VORPAL_SHELL_BACKUP_PATH".to_string(),
        "unset VORPAL_SHELL_BACKUP_PS1".to_string(),
        "unset VORPAL_SHELL_BACKUP_VORPAL_SHELL".to_string(),
    ];

    for env in environments.clone().into_iter() {
        let key = env.split("=").next().unwrap();

        if key == "PATH" {
            continue;
        }

        envs_backup.push(format!("export VORPAL_SHELL_BACKUP_{}=\"${}\"", key, key));
        envs_export.push(format!("export {}", env));
        envs_restore.push(format!("export {}=\"$VORPAL_SHELL_BACKUP_{}\"", key, key));
        envs_unset.push(format!("unset VORPAL_SHELL_BACKUP_{}", key));
    }

    // Setup path

    let step_path_artifacts = artifacts
        .iter()
        .map(|artifact| format!("{}/bin", get_env_key(artifact)))
        .collect::<Vec<String>>()
        .join(":");

    let mut step_path = step_path_artifacts;

    if let Some(path) = environments.iter().find(|x| x.starts_with("PATH=")) {
        if let Some(path_value) = path.split('=').nth(1) {
            step_path = format!("{path_value}:{step_path}");
        }
    }

    envs_export.push(format!("export PATH={step_path}:$PATH"));

    // Setup script

    let step_script = formatdoc! {"
        mkdir -pv $VORPAL_WORKSPACE/bin

        cat > bin/activate << \"EOF\"
        #!/bin/bash

        {backups}
        {exports}

        deactivate(){{
        {restores}
        {unsets}
        }}

        exec \"$@\"
        EOF

        chmod +x $VORPAL_WORKSPACE/bin/activate

        mkdir -pv $VORPAL_OUTPUT/bin

        cp -prv bin \"$VORPAL_OUTPUT\"",
        backups = envs_backup.join("\n"),
        exports = envs_export.join("\n"),
        restores = envs_restore.join("\n"),
        unsets = envs_unset.join("\n"),
    };

    let steps = vec![step::shell(context, artifacts, vec![], step_script).await?];

    ArtifactBuilder::new(name, steps, systems)
        .build(context)
        .await
}
