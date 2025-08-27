use crate::{api::artifact::ArtifactSystem, artifact::script, context::ConfigContext};
use anyhow::Result;

pub struct UserenvBuilder<'a> {
    pub artifacts: Vec<String>,
    pub environments: Vec<String>,
    pub name: &'a str,
    pub symlinks: Vec<(String, String)>,
    pub systems: Vec<ArtifactSystem>,
}

impl<'a> UserenvBuilder<'a> {
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
        script::userenv(
            context,
            self.artifacts,
            self.environments,
            self.name,
            self.symlinks,
            self.systems,
        )
        .await
    }
}
