use crate::{
    api::artifact::{ArtifactStepSecret, ArtifactSystem},
    artifact::script,
    context::ConfigContext,
};
use anyhow::Result;

pub struct DevenvBuilder<'a> {
    pub artifacts: Vec<String>,
    pub environments: Vec<String>,
    pub name: &'a str,
    pub secrets: Vec<ArtifactStepSecret>,
    pub systems: Vec<ArtifactSystem>,
}

impl<'a> DevenvBuilder<'a> {
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
        script::devenv(
            context,
            self.artifacts,
            self.environments,
            self.name,
            self.secrets,
            self.systems,
        )
        .await
    }
}
