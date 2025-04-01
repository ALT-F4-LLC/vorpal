use crate::context::ConfigContext;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use vorpal_schema::config::v0::{
    ConfigArtifact, ConfigArtifactSource, ConfigArtifactStep, ConfigArtifactSystem,
};

pub mod language;
pub mod shell;
pub mod step;

pub struct ConfigArtifactSourceBuilder {
    pub excludes: Vec<String>,
    pub hash: Option<String>,
    pub includes: Vec<String>,
    pub name: String,
    pub path: String,
}

pub struct ConfigArtifactStepBuilder {
    pub arguments: HashMap<ConfigArtifactSystem, Vec<String>>,
    pub artifacts: HashMap<ConfigArtifactSystem, Vec<String>>,
    pub entrypoint: HashMap<ConfigArtifactSystem, String>,
    pub environments: HashMap<ConfigArtifactSystem, Vec<String>>,
    pub script: HashMap<ConfigArtifactSystem, String>,
}

pub struct ConfigArtifactBuilder {
    pub name: String,
    pub sources: Vec<ConfigArtifactSource>,
    pub steps: Vec<ConfigArtifactStep>,
    pub systems: Vec<ConfigArtifactSystem>,
}

impl ConfigArtifactSourceBuilder {
    pub fn new(name: String, path: String) -> Self {
        Self {
            excludes: vec![],
            hash: None,
            includes: vec![],
            name,
            path,
        }
    }

    pub fn with_excludes(mut self, excludes: Vec<String>) -> Self {
        self.excludes = excludes;
        self
    }

    pub fn with_hash(mut self, hash: String) -> Self {
        self.hash = Some(hash);
        self
    }

    pub fn with_includes(mut self, includes: Vec<String>) -> Self {
        self.includes = includes;
        self
    }

    pub fn build(self) -> ConfigArtifactSource {
        ConfigArtifactSource {
            includes: self.includes,
            excludes: self.excludes,
            hash: self.hash,
            name: self.name,
            path: self.path,
        }
    }
}

impl ConfigArtifactStepBuilder {
    pub fn new() -> Self {
        Self {
            arguments: HashMap::new(),
            artifacts: HashMap::new(),
            entrypoint: HashMap::new(),
            environments: HashMap::new(),
            script: HashMap::new(),
        }
    }

    pub fn with_arguments(
        mut self,
        arguments: Vec<String>,
        systems: Vec<ConfigArtifactSystem>,
    ) -> Self {
        for system in systems {
            self.arguments.insert(system, arguments.clone());
        }

        self
    }

    pub fn with_artifacts(
        mut self,
        artifacts: Vec<String>,
        systems: Vec<ConfigArtifactSystem>,
    ) -> Self {
        for system in systems {
            self.artifacts.insert(system, artifacts.clone());
        }

        self
    }

    pub fn with_entrypoint(mut self, entrypoint: &str, systems: Vec<ConfigArtifactSystem>) -> Self {
        for system in systems {
            self.entrypoint.insert(system, entrypoint.to_string());
        }

        self
    }

    pub fn with_environments(
        mut self,
        environments: Vec<String>,
        systems: Vec<ConfigArtifactSystem>,
    ) -> Self {
        for system in systems {
            self.environments.insert(system, environments.clone());
        }

        self
    }

    pub fn with_script(mut self, script: String, systems: Vec<ConfigArtifactSystem>) -> Self {
        for system in systems {
            self.script.insert(system, script.clone());
        }

        self
    }

    pub fn build(self, context: &mut ConfigContext) -> ConfigArtifactStep {
        let system = context.get_target();

        ConfigArtifactStep {
            arguments: self.arguments.get(&system).unwrap_or(&vec![]).clone(),
            artifacts: self.artifacts.get(&system).unwrap_or(&vec![]).clone(),
            entrypoint: self.entrypoint.get(&system).cloned(),
            environments: self.environments.get(&system).unwrap_or(&vec![]).clone(),
            script: self.script.get(&system).cloned(),
        }
    }
}

impl ConfigArtifactBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            sources: vec![],
            steps: vec![],
            systems: vec![],
        }
    }

    pub fn with_source(mut self, source: ConfigArtifactSource) -> Self {
        if !self.sources.contains(&source) {
            self.sources.push(source);
        }

        self
    }

    pub fn with_step(mut self, step: ConfigArtifactStep) -> Self {
        if !self.steps.contains(&step) {
            self.steps.push(step);
        }

        self
    }

    pub fn with_system(mut self, system: ConfigArtifactSystem) -> Self {
        if !self.systems.contains(&system) {
            self.systems.push(system);
        }

        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let artifact = ConfigArtifact {
            name: self.name,
            sources: self.sources,
            steps: self.steps,
            systems: self.systems.into_iter().map(|v| v.into()).collect(),
            target: context.get_target().into(),
        };

        if artifact.steps.is_empty() {
            return Err(anyhow!("artifact must have at least one step"));
        }

        context.add_artifact(artifact).await
    }
}

pub fn get_env_key(hash: &str) -> String {
    format!("$VORPAL_ARTIFACT_{}", hash)
}
