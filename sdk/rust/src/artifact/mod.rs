use crate::{
    api::artifact::{
        Artifact, ArtifactSource, ArtifactStep, ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    context::ConfigContext,
};
use anyhow::{anyhow, bail, Result};
use indoc::formatdoc;
use std::collections::HashMap;

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
pub mod nginx;
pub mod protoc;
pub mod protoc_gen_go;
pub mod protoc_gen_go_grpc;
pub mod rust_analyzer;
pub mod rust_src;
pub mod rust_std;
pub mod rust_toolchain;
pub mod rustc;
pub mod rustfmt;
pub mod script;
pub mod staticcheck;
pub mod step;

pub struct ArtifactProcessBuilder<'a> {
    pub arguments: Vec<String>,
    pub artifacts: Vec<String>,
    pub entrypoint: &'a str,
    pub name: &'a str,
}

pub struct ArtifactSourceBuilder<'a> {
    pub digest: Option<&'a str>,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub name: &'a str,
    pub path: &'a str,
}

pub struct ArtifactStepBuilder {
    pub arguments: HashMap<ArtifactSystem, Vec<String>>,
    pub artifacts: HashMap<ArtifactSystem, Vec<String>>,
    pub entrypoint: HashMap<ArtifactSystem, String>,
    pub environments: HashMap<ArtifactSystem, Vec<String>>,
    pub script: HashMap<ArtifactSystem, String>,
}

pub struct ArtifactTaskBuilder<'a> {
    pub artifacts: Vec<String>,
    pub name: &'a str,
    pub script: String,
}

pub struct ArtifactVariableBuilder<'a> {
    pub encrypt: bool,
    pub name: &'a str,
    pub require: bool,
}

pub struct ArtifactBuilder<'a> {
    pub name: &'a str,
    pub sources: Vec<ArtifactSource>,
    pub steps: Vec<ArtifactStep>,
    pub systems: Vec<ArtifactSystem>,
}

impl<'a> ArtifactProcessBuilder<'a> {
    pub fn new(name: &'a str, entrypoint: &'a str) -> Self {
        Self {
            arguments: vec![],
            artifacts: vec![],
            entrypoint,
            name,
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
                .map(|v| format!("$VORPAL_ARTIFACT_{}/bin", v))
                .collect::<Vec<String>>()
                .join(":"),
            entrypoint = self.entrypoint,
            name = self.name,
        };

        let step = step::shell(context, self.artifacts, vec![], script).await?;

        ArtifactBuilder::new(self.name)
            .with_step(step)
            .with_system(Aarch64Darwin)
            .with_system(Aarch64Linux)
            .with_system(X8664Darwin)
            .with_system(X8664Linux)
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

impl Default for ArtifactStepBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactStepBuilder {
    pub fn new() -> Self {
        Self {
            arguments: HashMap::new(),
            artifacts: HashMap::new(),
            entrypoint: HashMap::new(),
            environments: HashMap::new(),
            script: HashMap::new(),
        }
    }

    pub fn with_arguments(mut self, arguments: Vec<&str>, systems: Vec<ArtifactSystem>) -> Self {
        for system in systems {
            self.arguments
                .insert(system, arguments.iter().map(|v| v.to_string()).collect());
        }

        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>, systems: Vec<ArtifactSystem>) -> Self {
        for system in systems {
            self.artifacts.insert(system, artifacts.clone());
        }

        self
    }

    pub fn with_entrypoint(mut self, entrypoint: &str, systems: Vec<ArtifactSystem>) -> Self {
        for system in systems {
            self.entrypoint.insert(system, entrypoint.to_string());
        }

        self
    }

    pub fn with_environments(
        mut self,
        environments: Vec<String>,
        systems: Vec<ArtifactSystem>,
    ) -> Self {
        for system in systems {
            self.environments.insert(system, environments.clone());
        }

        self
    }

    pub fn with_script(mut self, script: String, systems: Vec<ArtifactSystem>) -> Self {
        for system in systems {
            self.script.insert(system, script.clone());
        }

        self
    }

    pub fn build(self, context: &mut ConfigContext) -> ArtifactStep {
        let system = context.get_system();

        ArtifactStep {
            arguments: self.arguments.get(&system).unwrap_or(&vec![]).clone(),
            artifacts: self.artifacts.get(&system).unwrap_or(&vec![]).clone(),
            entrypoint: self.entrypoint.get(&system).cloned(),
            environments: self.environments.get(&system).unwrap_or(&vec![]).clone(),
            script: self.script.get(&system).cloned(),
        }
    }
}

impl<'a> ArtifactTaskBuilder<'a> {
    pub fn new(name: &'a str, script: String) -> Self {
        Self {
            artifacts: vec![],
            name,
            script,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        ArtifactBuilder::new(self.name)
            .with_step(step::shell(context, self.artifacts, vec![], self.script).await?)
            .with_system(Aarch64Darwin)
            .with_system(Aarch64Linux)
            .with_system(X8664Darwin)
            .with_system(X8664Linux)
            .build(context)
            .await
    }
}

impl<'a> ArtifactBuilder<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            sources: vec![],
            steps: vec![],
            systems: vec![],
            // variables: vec![],
        }
    }

    pub fn with_source(mut self, source: ArtifactSource) -> Self {
        if !self.sources.contains(&source) {
            self.sources.push(source);
        }

        self
    }

    pub fn with_step(mut self, step: ArtifactStep) -> Self {
        if !self.steps.contains(&step) {
            self.steps.push(step);
        }

        self
    }

    pub fn with_system(mut self, system: ArtifactSystem) -> Self {
        if !self.systems.contains(&system) {
            self.systems.push(system);
        }

        self
    }

    // pub fn with_variable(mut self, variable: ArtifactVariable) -> Self {
    //     if !self.variables.contains(&variable) {
    //         self.variables.push(variable);
    //     }
    //
    //     self
    // }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let artifact = Artifact {
            name: self.name.to_string(),
            sources: self.sources,
            steps: self.steps,
            systems: self.systems.into_iter().map(|v| v.into()).collect(),
            target: context.get_system().into(),
            // variables: self.variables,
        };

        if artifact.steps.is_empty() {
            return Err(anyhow!("must have at least one step"));
        }

        context.add_artifact(&artifact).await
    }
}

impl<'a> ArtifactVariableBuilder<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            encrypt: false,
            name,
            require: false,
        }
    }

    pub fn with_encrypt(mut self) -> Self {
        self.encrypt = true;
        self
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

pub fn get_env_key(digest: &String) -> String {
    format!("$VORPAL_ARTIFACT_{}", digest)
}
