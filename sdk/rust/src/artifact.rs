use crate::{
    api::artifact::{Artifact, ArtifactSource, ArtifactStep, ArtifactStepSecret, ArtifactSystem},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub mod cargo;
pub mod clippy;
pub mod gh;
pub mod go;
pub mod goimports;
pub mod gopls;
pub mod grpcurl;
pub mod language;
pub mod linux_debian;
pub mod linux_vorpal;
pub mod project_environment;
pub mod protoc;
pub mod protoc_gen_go;
pub mod protoc_gen_go_grpc;
pub mod rust_analyzer;
pub mod rust_src;
pub mod rust_std;
pub mod rust_toolchain;
pub mod rustc;
pub mod rustfmt;
pub mod source;
pub mod staticcheck;
pub mod step;
pub mod system;
pub mod user_environment;

pub struct ArgumentBuilder<'a> {
    pub name: &'a str,
    pub require: bool,
}

pub struct ProcessBuilder<'a> {
    pub arguments: Vec<String>,
    pub artifacts: Vec<String>,
    pub entrypoint: &'a str,
    pub name: &'a str,
    pub secrets: Vec<ArtifactStepSecret>,
    pub systems: Vec<ArtifactSystem>,
}

pub struct ArtifactSourceBuilder<'a> {
    pub digest: Option<&'a str>,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub name: &'a str,
    pub path: &'a str,
}

pub struct ArtifactStepBuilder<'a> {
    pub arguments: Vec<String>,
    pub artifacts: Vec<String>,
    pub entrypoint: &'a str,
    pub environments: Vec<String>,
    pub secrets: Vec<ArtifactStepSecret>,
    pub script: Option<String>,
}

pub struct JobBuilder<'a> {
    pub artifacts: Vec<String>,
    pub name: &'a str,
    pub secrets: Vec<ArtifactStepSecret>,
    pub script: String,
    pub systems: Vec<ArtifactSystem>,
}

pub struct ArtifactBuilder<'a> {
    pub aliases: Vec<String>,
    pub name: &'a str,
    pub sources: Vec<ArtifactSource>,
    pub steps: Vec<ArtifactStep>,
    pub systems: Vec<ArtifactSystem>,
}

impl<'a> ArgumentBuilder<'a> {
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

    pub fn build(self, context: &mut ConfigContext) -> Result<Option<String>> {
        let variable = context.get_variable(self.name);

        if self.require && variable.is_none() {
            bail!("variable '{}' is required", self.name)
        }

        Ok(variable)
    }
}

impl<'a> ProcessBuilder<'a> {
    pub fn new(name: &'a str, entrypoint: &'a str, systems: Vec<ArtifactSystem>) -> Self {
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
                self.secrets.push(ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
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

        ArtifactBuilder::new(self.name, vec![step], self.systems)
            .build(context)
            .await
    }
}
impl<'a> ArtifactSourceBuilder<'a> {
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

    pub fn build(self) -> ArtifactSource {
        ArtifactSource {
            digest: self.digest.map(|v| v.to_string()),
            includes: self.includes,
            excludes: self.excludes,
            name: self.name.to_string(),
            path: self.path.to_string(),
        }
    }
}

impl<'a> ArtifactStepBuilder<'a> {
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

    pub fn with_secrets(mut self, secrets: Vec<ArtifactStepSecret>) -> Self {
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

    pub fn build(self) -> ArtifactStep {
        ArtifactStep {
            arguments: self.arguments,
            artifacts: self.artifacts,
            entrypoint: Some(self.entrypoint.to_string()),
            environments: self.environments,
            secrets: self.secrets,
            script: self.script,
        }
    }
}

impl<'a> JobBuilder<'a> {
    pub fn new(name: &'a str, script: String, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            name,
            secrets: vec![],
            script,
            systems,
        }
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
                self.secrets.push(ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let step = step::shell(context, self.artifacts, vec![], self.script, self.secrets).await?;

        ArtifactBuilder::new(self.name, vec![step], self.systems)
            .build(context)
            .await
    }
}

impl<'a> ArtifactBuilder<'a> {
    pub fn new(name: &'a str, steps: Vec<ArtifactStep>, systems: Vec<ArtifactSystem>) -> Self {
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

    pub fn with_sources(mut self, sources: Vec<ArtifactSource>) -> Self {
        for source in sources {
            if !self.sources.iter().any(|s| s.name == source.name) {
                self.sources.push(source);
            }
        }

        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let artifact = Artifact {
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

pub fn get_env_key(digest: &String) -> String {
    format!("$VORPAL_ARTIFACT_{digest}")
}
