use crate::config::{
    artifact::{add_artifact, get_artifact_envkey},
    ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

pub async fn shell_artifact<'a>(
    context: &mut ConfigContext,
    artifacts: Vec<ArtifactId>,
    environments: Vec<&'a str>,
    name: &'a str,
) -> Result<ArtifactId> {
    let mut env_paths = vec![];

    for artifact in artifacts.iter() {
        env_paths.push(format!("{}/bin", get_artifact_envkey(artifact)));
    }

    let mut backups = vec![
        "export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"".to_string(),
        "export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"".to_string(),
        "export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"".to_string(),
    ];

    let mut exports = vec![
        format!("export PATH=\"{}:$PATH\"", env_paths.join(":")),
        format!("export PS1=\"({}) $PS1\"", name),
        "export VORPAL_SHELL=\"1\"".to_string(),
    ];

    let mut restores = vec![
        "export PATH=\"$VORPAL_SHELL_BACKUP_PATH\"".to_string(),
        "export PS1=\"$VORPAL_SHELL_BACKUP_PS1\"".to_string(),
        "export VORPAL_SHELL=\"$VORPAL_SHELL_BACKUP_VORPAL_SHELL\"".to_string(),
    ];

    let mut unsets = vec![
        "unset VORPAL_SHELL_BACKUP_PATH".to_string(),
        "unset VORPAL_SHELL_BACKUP_PS1".to_string(),
        "unset VORPAL_SHELL_BACKUP_VORPAL_SHELL".to_string(),
    ];

    for env in environments {
        let key = env.split("=").next().unwrap();
        backups.push(format!("export VORPAL_SHELL_BACKUP_{}=\"${}\"", key, key));
        exports.push(format!("export {}", env));
        restores.push(format!("export {}=\"$VORPAL_SHELL_BACKUP_{}\"", key, key));
        unsets.push(format!("unset VORPAL_SHELL_BACKUP_{}", key));
    }

    add_artifact(
        context,
        artifacts,
        vec![],
        name,
        formatdoc! {"
            mkdir -pv $VORPAL_WORKSPACE/bin

            cat > bin/deactivate << 'EOF'
            #!/bin/bash
            # Set restore variables
            {restores}

            # Set unset variables
            {unsets}
            EOF

            cat > bin/activate << 'EOF'
            #!/bin/bash

            # Set backup variables
            {backups}

            # Set new variables
            {exports}

            # Restore old variables
            alias exit-shell='source $VORPAL_OUTPUT/bin/deactivate'

            # Run the command
            exec \"$@\"
            EOF

            chmod +x \
                $VORPAL_WORKSPACE/bin/activate \
                $VORPAL_WORKSPACE/bin/deactivate

            mkdir -pv $VORPAL_OUTPUT/bin

            cp -prv bin \"$VORPAL_OUTPUT\"",
            backups = backups.join("\n"),
            exports = exports.join("\n"),
            restores = restores.join("\n"),
            unsets = unsets.join("\n"),
        },
        vec![],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}
