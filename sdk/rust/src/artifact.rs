use crate::{api, context};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub mod cargo;
pub mod clippy;
pub mod crane;
pub mod gh;
pub mod git;
pub mod go;
pub mod goimports;
pub mod gopls;
pub mod grpcurl;
pub mod language;
pub mod linux_debian;
pub mod linux_vorpal;
pub mod linux_vorpal_slim;
pub mod oci_image;
pub mod protoc;
pub mod protoc_gen_go;
pub mod protoc_gen_go_grpc;
pub mod rsync;
pub mod rust_analyzer;
pub mod rust_src;
pub mod rust_std;
pub mod rust_toolchain;
pub mod rustc;
pub mod rustfmt;
pub mod staticcheck;
pub mod step;
pub mod system;

pub struct Argument<'a> {
    pub name: &'a str,
    pub require: bool,
}

pub struct ArtifactSource<'a> {
    pub digest: Option<&'a str>,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub name: &'a str,
    pub path: &'a str,
}

pub struct ArtifactStep<'a> {
    pub arguments: Vec<String>,
    pub artifacts: Vec<String>,
    pub entrypoint: &'a str,
    pub environments: Vec<String>,
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    pub script: Option<String>,
}

pub struct Artifact<'a> {
    pub aliases: Vec<String>,
    pub name: &'a str,
    pub sources: Vec<api::artifact::ArtifactSource>,
    pub steps: Vec<api::artifact::ArtifactStep>,
    pub systems: Vec<api::artifact::ArtifactSystem>,
}

pub struct Job<'a> {
    pub artifacts: Vec<String>,
    pub name: &'a str,
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    pub script: String,
    pub systems: Vec<api::artifact::ArtifactSystem>,
}

pub struct Process<'a> {
    pub arguments: Vec<String>,
    pub artifacts: Vec<String>,
    pub entrypoint: &'a str,
    pub name: &'a str,
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    pub systems: Vec<api::artifact::ArtifactSystem>,
}

pub struct ProjectEnvironment<'a> {
    pub artifacts: Vec<String>,
    pub environments: Vec<String>,
    pub name: &'a str,
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    pub systems: Vec<api::artifact::ArtifactSystem>,
}

pub struct UserEnvironment<'a> {
    pub artifacts: Vec<String>,
    pub environments: Vec<String>,
    pub name: &'a str,
    pub symlinks: Vec<(String, String)>,
    pub systems: Vec<api::artifact::ArtifactSystem>,
}

impl<'a> Argument<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            require: false,
        }
    }

    pub fn with_require(mut self) -> Self {
        self.require = true;
        self
    }

    pub fn build(self, context: &mut context::ConfigContext) -> Result<Option<String>> {
        let variable = context.get_variable(self.name);

        if self.require && variable.is_none() {
            bail!("variable '{}' is required", self.name)
        }

        Ok(variable)
    }
}

impl<'a> ArtifactSource<'a> {
    pub fn new(name: &'a str, path: &'a str) -> Self {
        Self {
            digest: None,
            excludes: vec![],
            includes: vec![],
            name,
            path,
        }
    }

    pub fn with_digest(mut self, digest: &'a str) -> Self {
        self.digest = Some(digest);
        self
    }

    pub fn with_excludes(mut self, excludes: Vec<String>) -> Self {
        self.excludes = excludes;
        self
    }

    pub fn with_includes(mut self, includes: Vec<String>) -> Self {
        self.includes = includes;
        self
    }

    pub fn build(self) -> api::artifact::ArtifactSource {
        api::artifact::ArtifactSource {
            digest: self.digest.map(|v| v.to_string()),
            includes: self.includes,
            excludes: self.excludes,
            name: self.name.to_string(),
            path: self.path.to_string(),
        }
    }
}

impl<'a> ArtifactStep<'a> {
    pub fn new(entrypoint: &'a str) -> Self {
        Self {
            arguments: vec![],
            artifacts: vec![],
            entrypoint,
            environments: vec![],
            secrets: vec![],
            script: None,
        }
    }

    pub fn with_arguments(mut self, arguments: Vec<&str>) -> Self {
        self.arguments = arguments.iter().map(|v| v.to_string()).collect();
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments = environments;
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<api::artifact::ArtifactStepSecret>) -> Self {
        for secret in secrets {
            if !self.secrets.iter().any(|s| s.name == secret.name) {
                self.secrets.push(secret);
            }
        }
        self
    }

    pub fn with_script(mut self, script: String) -> Self {
        self.script = Some(script);
        self
    }

    pub fn build(self) -> api::artifact::ArtifactStep {
        api::artifact::ArtifactStep {
            arguments: self.arguments,
            artifacts: self.artifacts,
            entrypoint: Some(self.entrypoint.to_string()),
            environments: self.environments,
            secrets: self.secrets,
            script: self.script,
        }
    }
}

impl<'a> Artifact<'a> {
    pub fn new(
        name: &'a str,
        steps: Vec<api::artifact::ArtifactStep>,
        systems: Vec<api::artifact::ArtifactSystem>,
    ) -> Self {
        Self {
            aliases: vec![],
            name,
            sources: vec![],
            steps,
            systems,
        }
    }

    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        for alias in aliases {
            if !self.aliases.contains(&alias) {
                self.aliases.push(alias);
            }
        }
        self
    }

    pub fn with_sources(mut self, sources: Vec<api::artifact::ArtifactSource>) -> Self {
        for source in sources {
            if !self.sources.iter().any(|s| s.name == source.name) {
                self.sources.push(source);
            }
        }

        self
    }

    pub async fn build(self, context: &mut context::ConfigContext) -> Result<String> {
        let artifact = api::artifact::Artifact {
            aliases: self.aliases,
            name: self.name.to_string(),
            sources: self.sources,
            steps: self.steps,
            systems: self.systems.into_iter().map(|v| v.into()).collect(),
            target: context.get_system().into(),
        };

        context.add_artifact(&artifact).await
    }
}

impl<'a> Job<'a> {
    pub fn new(name: &'a str, script: String, systems: Vec<api::artifact::ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            name,
            secrets: vec![],
            script,
            systems,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(&str, &str)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(api::artifact::ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let step = step::shell(context, self.artifacts, vec![], self.script, self.secrets).await?;

        Artifact::new(self.name, vec![step], self.systems)
            .build(context)
            .await
    }
}

impl<'a> ProjectEnvironment<'a> {
    pub fn new(name: &'a str, systems: Vec<api::artifact::ArtifactSystem>) -> Self {
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
                self.secrets.push(api::artifact::ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

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

        Artifact::new(self.name, steps, self.systems)
            .build(context)
            .await
    }
}

impl<'a> Process<'a> {
    pub fn new(
        name: &'a str,
        entrypoint: &'a str,
        systems: Vec<api::artifact::ArtifactSystem>,
    ) -> Self {
        Self {
            arguments: vec![],
            artifacts: vec![],
            entrypoint,
            name,
            secrets: vec![],
            systems,
        }
    }

    pub fn with_arguments(mut self, arguments: Vec<&str>) -> Self {
        self.arguments = arguments.iter().map(|v| v.to_string()).collect();
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        for artifact in artifacts {
            if !self.artifacts.contains(&artifact) {
                self.artifacts.push(artifact);
            }
        }
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(&str, &str)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(api::artifact::ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let script = formatdoc! {r#"
            mkdir -pv $VORPAL_OUTPUT/bin

            cat > $VORPAL_OUTPUT/bin/{name}-logs << "EOF"
            #!/bin/bash
            set -euo pipefail

            if [ -f $VORPAL_OUTPUT/logs.txt ]; then
                tail -f $VORPAL_OUTPUT/logs.txt
            else
                echo "No logs found"
            fi
            EOF

            chmod +x $VORPAL_OUTPUT/bin/{name}-logs

            cat > $VORPAL_OUTPUT/bin/{name}-stop << "EOF"
            #!/bin/bash
            set -euo pipefail

            if [ -f $VORPAL_OUTPUT/pid ]; then
                kill $(cat $VORPAL_OUTPUT/pid)
                rm -rf $VORPAL_OUTPUT/pid
            fi
            EOF

            chmod +x $VORPAL_OUTPUT/bin/{name}-stop

            cat > $VORPAL_OUTPUT/bin/{name}-start << "EOF"
            #!/bin/bash
            set -euo pipefail

            export PATH={artifacts}:$PATH

            $VORPAL_OUTPUT/bin/{name}-stop

            echo "Process: {entrypoint} {arguments}"

            nohup {entrypoint} {arguments} > $VORPAL_OUTPUT/logs.txt 2>&1 &

            PROCESS_PID=$!

            echo "Process ID: $PROCESS_PID"

            echo $PROCESS_PID > $VORPAL_OUTPUT/pid

            echo "Process commands:"
            echo "- {name}-logs (tail logs)"
            echo "- {name}-stop (stop process)"
            echo "- {name}-start (start process)"
            EOF

            chmod +x $VORPAL_OUTPUT/bin/{name}-start"#,
            arguments = self
                .arguments
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<String>>()
                .join(" "),
            artifacts = self
                .artifacts
                .iter()
                .map(|v| format!("$VORPAL_ARTIFACT_{v}/bin"))
                .collect::<Vec<String>>()
                .join(":"),
            entrypoint = self.entrypoint,
            name = self.name,
        };

        let step = step::shell(context, self.artifacts, vec![], script, self.secrets).await?;

        Artifact::new(self.name, vec![step], self.systems)
            .build(context)
            .await
    }
}

impl<'a> UserEnvironment<'a> {
    pub fn new(name: &'a str, systems: Vec<api::artifact::ArtifactSystem>) -> Self {
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

    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        // Sort for deterministic output
        self.symlinks.sort_by(|a, b| a.0.cmp(&b.0));

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

        Artifact::new(self.name, steps, self.systems)
            .build(context)
            .await
    }
}

pub fn get_default_address() -> String {
    if let Ok(path) = std::env::var("VORPAL_SOCKET_PATH") {
        if !path.is_empty() {
            return format!("unix://{}", path);
        }
    }
    "unix:///var/lib/vorpal/vorpal.sock".to_string()
}

pub fn get_env_key(digest: &String) -> String {
    format!("$VORPAL_ARTIFACT_{digest}")
}
