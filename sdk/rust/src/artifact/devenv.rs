use crate::{
    api::artifact::{ArtifactStepSecret, ArtifactSystem},
    artifact::{get_env_key, step, ArtifactBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

pub struct DevEnvBuilder<'a> {
    pub artifacts: Vec<String>,
    pub environments: Vec<String>,
    pub name: &'a str,
    pub secrets: Vec<ArtifactStepSecret>,
    pub systems: Vec<ArtifactSystem>,
}

impl<'a> DevEnvBuilder<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            environments: vec![],
            name,
            secrets: vec![],
            systems,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments = environments;
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(&str, &str)>) -> Self {
        for (name, value) in secrets.into_iter() {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let mut envs_backup = vec![
            "export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"".to_string(),
            "export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"".to_string(),
            "export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"".to_string(),
        ];

        let mut envs_export = vec![
            format!("export PS1=\"({}) $PS1\"", self.name),
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

        for env in self.environments.clone().into_iter() {
            let key = env.split("=").next().unwrap();

            if key == "PATH" {
                continue;
            }

            envs_backup.push(format!("export VORPAL_SHELL_BACKUP_{key}=\"${key}\""));
            envs_export.push(format!("export {env}"));
            envs_restore.push(format!("export {key}=\"$VORPAL_SHELL_BACKUP_{key}\""));
            envs_unset.push(format!("unset VORPAL_SHELL_BACKUP_{key}"));
        }

        // Setup path

        let step_path_artifacts = self
            .artifacts
            .iter()
            .map(|artifact| format!("{}/bin", get_env_key(artifact)))
            .collect::<Vec<String>>()
            .join(":");

        let mut step_path = step_path_artifacts;

        if let Some(path) = self.environments.iter().find(|x| x.starts_with("PATH=")) {
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

        let steps =
            vec![step::shell(context, self.artifacts, vec![], step_script, self.secrets).await?];

        ArtifactBuilder::new(self.name, steps, self.systems)
            .build(context)
            .await
    }
}
