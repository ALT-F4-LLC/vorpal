use crate::{
    api,
    api::artifact::ArtifactSystem,
    artifact::{bun::Bun, get_env_key, step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

pub struct TypeScript<'a> {
    artifacts: Vec<String>,
    bun_version: Option<String>,
    entrypoint: Option<&'a str>,
    environments: Vec<&'a str>,
    includes: Vec<&'a str>,
    name: &'a str,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    systems: Vec<ArtifactSystem>,
    working_dir: Option<String>,
}

impl<'a> TypeScript<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            bun_version: None,
            entrypoint: None,
            environments: vec![],
            includes: vec![],
            name,
            secrets: vec![],
            systems,
            working_dir: None,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_bun_version(mut self, version: &str) -> Self {
        self.bun_version = Some(version.to_string());
        self
    }

    pub fn with_entrypoint(mut self, entrypoint: &'a str) -> Self {
        self.entrypoint = Some(entrypoint);
        self
    }

    pub fn with_environments(mut self, environments: Vec<&'a str>) -> Self {
        self.environments = environments;
        self
    }

    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.includes = includes;
        self
    }

    pub fn with_working_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(String, String)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets
                    .push(api::artifact::ArtifactStepSecret { name, value });
            }
        }

        self
    }

    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let source_path = ".";

        let mut source_builder = ArtifactSource::new(self.name, source_path);

        if !self.includes.is_empty() {
            let source_includes = self.includes.iter().map(|s| s.to_string()).collect();
            source_builder = source_builder.with_includes(source_includes);
        }

        let source = source_builder.build();

        let source_dir = format!("./source/{}", source.name);

        let work_dir = match &self.working_dir {
            Some(dir) => format!("{}/{}", source_dir, dir),
            None => source_dir.clone(),
        };

        let entrypoint = match self.entrypoint {
            Some(ep) => ep.to_string(),
            None => format!("src/{}.ts", self.name),
        };

        let mut bun = Bun::new();
        if let Some(version) = &self.bun_version {
            bun = bun.with_version(version);
        }
        let bun = bun.build(context).await?;
        let bun_bin = format!("{}/bin", get_env_key(&bun));

        // NOTE: Pre-flight validation (package.json, bun.lockb, entrypoint, @vorpal/sdk)
        // is performed by the CLI in cli/src/command/build.rs before this build step runs.
        let step_script = formatdoc! {r#"
            set -euo pipefail

            pushd {work_dir}

            mkdir -p $VORPAL_OUTPUT/bin

            {bun_bin}/bun install --frozen-lockfile
            {bun_bin}/bun build --compile {entrypoint} --outfile $VORPAL_OUTPUT/bin/{name}"#,
            name = self.name,
        };

        let mut step_environments = vec![
            format!("PATH={bun_bin}"),
        ];

        for env in self.environments {
            step_environments.push(env.to_string());
        }

        let steps = vec![
            step::shell(
                context,
                [vec![bun.clone()], self.artifacts].concat(),
                step_environments,
                step_script,
                self.secrets,
            )
            .await?,
        ];

        Artifact::new(self.name, steps, self.systems)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
