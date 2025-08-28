use crate::{
    api::artifact::ArtifactSystem,
    artifact::{get_env_key, step, ArtifactBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

pub struct UserEnvBuilder<'a> {
    pub artifacts: Vec<String>,
    pub environments: Vec<String>,
    pub name: &'a str,
    pub symlinks: Vec<(String, String)>,
    pub systems: Vec<ArtifactSystem>,
}

impl<'a> UserEnvBuilder<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            environments: vec![],
            name,
            symlinks: vec![],
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

    pub fn with_symlinks(mut self, symlinks: Vec<(&str, &str)>) -> Self {
        for (source, target) in symlinks.into_iter() {
            self.symlinks.push((source.to_string(), target.to_string()));
        }
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
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

        // Setup script

        let step_script = formatdoc! {r#"
            mkdir -pv $VORPAL_OUTPUT/bin

            cat > $VORPAL_OUTPUT/bin/vorpal-activate-shell << "EOF"
            {environments}
            export PATH="$VORPAL_OUTPUT/bin:{step_path}:$PATH"
            EOF

            cat > $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks << "EOF"
            #!/bin/bash
            set -euo pipefail
            {symlinks_deactivate}
            EOF

            cat > $VORPAL_OUTPUT/bin/vorpal-activate-symlinks << "EOF"
            #!/bin/bash
            set -euo pipefail
            {symlinks_check}
            {symlinks_activate}
            EOF

            cat > $VORPAL_OUTPUT/bin/vorpal-activate << "EOF"
            #!/bin/bash
            set -euo pipefail

            echo "Deactivating previous symlinks..."

            if [ -f $HOME/.vorpal/bin/vorpal-deactivate-symlinks ]; then
                $HOME/.vorpal/bin/vorpal-deactivate-symlinks
            fi

            echo "Activating symlinks..."

            $VORPAL_OUTPUT/bin/vorpal-activate-symlinks

            echo "Vorpal userenv installed. Run 'source vorpal-activate-shell' to activate."

            ln -sfv $VORPAL_OUTPUT/bin/vorpal-activate-shell $HOME/.vorpal/bin/vorpal-activate-shell
            ln -sfv $VORPAL_OUTPUT/bin/vorpal-activate-symlinks $HOME/.vorpal/bin/vorpal-activate-symlinks
            ln -sfv $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks $HOME/.vorpal/bin/vorpal-deactivate-symlinks
            EOF


            chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-shell
            chmod +x $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks
            chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-symlinks
            chmod +x $VORPAL_OUTPUT/bin/vorpal-activate"#,
            environments = self.environments
                .iter()
                .filter(|e| !e.starts_with("PATH="))
                .map(|e| format!("export {e}"))
                .collect::<Vec<String>>()
                .join("\n"),
            symlinks_deactivate = self.symlinks
                .iter()
                .map(|(_, target)| format!("rm -fv {target}"))
                .collect::<Vec<String>>()
                .join("\n"),
            symlinks_check = self.symlinks
                .iter()
                .map(|(_, target)| format!("if [ -f {target} ]; then echo \"ERROR: Symlink target exists -> {target}\" && exit 1; fi"))
                .collect::<Vec<String>>()
                .join("\n"),
            symlinks_activate = self.symlinks
                .iter()
                .map(|(source, target)| format!("ln -sv {source} {target}"))
                .collect::<Vec<String>>()
                .join("\n"),
        };

        let steps = vec![step::shell(context, self.artifacts, vec![], step_script, vec![]).await?];

        ArtifactBuilder::new(self.name, steps, self.systems)
            .build(context)
            .await
    }
}
