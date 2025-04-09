use crate::{
    artifact::{get_env_key, step, ArtifactBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::artifact::v0::ArtifactSystem::{
    Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux,
};

pub async fn build<'a>(
    context: &mut ConfigContext,
    artifacts: Vec<String>,
    environments: Vec<String>,
    name: &'a str,
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

    for env in environments {
        let key = env.split("=").next().unwrap();

        envs_backup.push(format!("export VORPAL_SHELL_BACKUP_{}=\"${}\"", key, key));
        envs_export.push(format!("export {}", env));
        envs_restore.push(format!("export {}=\"$VORPAL_SHELL_BACKUP_{}\"", key, key));
        envs_unset.push(format!("unset VORPAL_SHELL_BACKUP_{}", key));
    }

    let path_artifacts = artifacts
        .iter()
        .map(|artifact| format!("{}/bin", get_env_key(artifact)))
        .collect::<Vec<String>>()
        .join(":");

    let path_export = format!("export PATH=\"{path_artifacts}:$PATH\"");

    envs_export.push(path_export);

    let step_script = formatdoc! {"
        mkdir -pv $VORPAL_WORKSPACE/bin

        cat > bin/activate << \"EOF\"
        #!/bin/bash

        # Set backup variables
        {backups}

        # Set new variables
        {exports}

        # Restore old variables
        exit-shell(){{
        # Set restore variables
        {restores}

        # Set unset variables
        {unsets}
        }}

        # Run the command
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

    let step = step::shell(context, artifacts, vec![], step_script).await?;

    ArtifactBuilder::new(name)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
