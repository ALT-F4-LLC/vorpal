use crate::{
    api,
    api::artifact::ArtifactSystem,
    artifact::{bun::Bun, get_env_key, step, Artifact, ArtifactSource, DevelopmentEnvironment},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use std::collections::BTreeMap;

pub struct TypeScript<'a> {
    artifacts: Vec<String>,
    bun_version: Option<String>,
    entrypoint: Option<&'a str>,
    environments: Vec<&'a str>,
    includes: Vec<&'a str>,
    name: &'a str,
    proto_artifact: Option<String>,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    systems: Vec<ArtifactSystem>,
    working_dir: Option<String>,
    node_modules: BTreeMap<String, String>,
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
            proto_artifact: None,
            secrets: vec![],
            systems,
            working_dir: None,
            node_modules: BTreeMap::new(),
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

    pub fn with_proto_artifact(mut self, artifact: String) -> Self {
        self.proto_artifact = Some(artifact);
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

    pub fn with_node_module(mut self, package_name: &str, digest: String) -> Self {
        self.node_modules.insert(package_name.to_string(), digest);
        self
    }

    pub fn with_node_modules(mut self, modules: Vec<(&str, String)>) -> Self {
        for (name, digest) in modules {
            self.node_modules.insert(name.to_string(), digest);
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

        // If a proto artifact is provided, copy its generated files into the source tree
        // before bun install so that TypeScript imports from ./api/ resolve correctly.
        let proto_copy_script = match &self.proto_artifact {
            Some(digest) => {
                let proto_env = get_env_key(digest);
                formatdoc! {r#"
                    cp -pr {proto_env}/api src/api
                "#}
            }
            None => String::new(),
        };

        // Generate node modules symlink script (BTreeMap iterates in sorted key order)
        let node_modules_script = if self.node_modules.is_empty() {
            String::new()
        } else {
            let mut lines = vec!["mkdir -p node_modules".to_string()];
            for (package_name, digest) in &self.node_modules {
                let env_key = get_env_key(digest);
                if package_name.contains('/') {
                    // Scoped package like @vorpal/sdk
                    let scope = package_name.split('/').next().unwrap();
                    lines.push(format!("mkdir -p node_modules/{scope}"));
                    lines.push(format!("ln -sf {env_key} node_modules/{package_name}"));
                } else {
                    // Unscoped package like lodash
                    lines.push(format!("ln -sf {env_key} node_modules/{package_name}"));
                }
            }
            lines.join("\n") + "\n"
        };

        let step_script = formatdoc! {r#"
            set -euo pipefail

            pushd {work_dir}

            mkdir -p $VORPAL_OUTPUT/bin
            {proto_copy_script}
            {node_modules_script}
            {bun_bin}/bun install --frozen-lockfile
            {bun_bin}/bun build --compile {entrypoint} --outfile ./{name}
            cp ./{name} $VORPAL_OUTPUT/bin/{name}
            rm ./{name}"#,
            name = self.name,
        };

        let mut step_environments = vec![format!("PATH={bun_bin}")];

        for env in self.environments {
            step_environments.push(env.to_string());
        }

        let mut step_artifacts = vec![bun.clone()];

        if let Some(ref proto) = self.proto_artifact {
            step_artifacts.push(proto.clone());
        }

        step_artifacts.extend(self.artifacts);

        for digest in self.node_modules.values() {
            step_artifacts.push(digest.clone());
        }

        let steps = vec![
            step::shell(
                context,
                step_artifacts,
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

// ---------------------------------------------------------------------------
// TypeScript Development Environment
// ---------------------------------------------------------------------------

pub struct TypeScriptDevelopmentEnvironment<'a> {
    artifacts: Vec<String>,
    environments: Vec<String>,
    name: &'a str,
    secrets: Vec<(&'a str, &'a str)>,
    systems: Vec<ArtifactSystem>,
}

impl<'a> TypeScriptDevelopmentEnvironment<'a> {
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
        self.artifacts.extend(artifacts);
        self
    }

    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments.extend(environments);
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(&'a str, &'a str)>) -> Self {
        for secret in secrets {
            if !self.secrets.iter().any(|(name, _)| *name == secret.0) {
                self.secrets.push(secret);
            }
        }
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let bun = Bun::new().build(context).await?;

        let mut artifacts = vec![bun];
        artifacts.extend(self.artifacts);

        let environments = self.environments;

        let mut devenv = DevelopmentEnvironment::new(self.name, self.systems)
            .with_artifacts(artifacts)
            .with_environments(environments);

        if !self.secrets.is_empty() {
            devenv = devenv.with_secrets(self.secrets);
        }

        devenv.build(context).await
    }
}

// ---------------------------------------------------------------------------
// TypeScript Library Builder
// ---------------------------------------------------------------------------

pub struct TypeScriptLibrary<'a> {
    artifacts: Vec<String>,
    build_command: Option<&'a str>,
    bun_version: Option<String>,
    environments: Vec<&'a str>,
    excludes: Vec<&'a str>,
    includes: Vec<&'a str>,
    name: &'a str,
    node_modules: BTreeMap<String, String>,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    source: Option<api::artifact::ArtifactSource>,
    source_scripts: Vec<String>,
    systems: Vec<ArtifactSystem>,
}

impl<'a> TypeScriptLibrary<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            build_command: None,
            bun_version: None,
            environments: vec![],
            excludes: vec![],
            includes: vec![],
            name,
            node_modules: BTreeMap::new(),
            secrets: vec![],
            source: None,
            source_scripts: vec![],
            systems,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_build_command(mut self, command: &'a str) -> Self {
        self.build_command = Some(command);
        self
    }

    pub fn with_bun_version(mut self, version: &str) -> Self {
        self.bun_version = Some(version.to_string());
        self
    }

    pub fn with_environments(mut self, environments: Vec<&'a str>) -> Self {
        self.environments = environments;
        self
    }

    pub fn with_excludes(mut self, excludes: Vec<&'a str>) -> Self {
        self.excludes = excludes;
        self
    }

    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.includes = includes;
        self
    }

    pub fn with_node_module(mut self, package_name: &str, digest: String) -> Self {
        self.node_modules.insert(package_name.to_string(), digest);
        self
    }

    pub fn with_node_modules(mut self, modules: Vec<(&str, String)>) -> Self {
        for (name, digest) in modules {
            self.node_modules.insert(name.to_string(), digest);
        }
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

    pub fn with_source(mut self, source: api::artifact::ArtifactSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_source_script(mut self, script: String) -> Self {
        if !self.source_scripts.contains(&script) {
            self.source_scripts.push(script);
        }
        self
    }

    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        // Build source
        let source = if let Some(source) = self.source.take() {
            source
        } else {
            let source_path = ".";
            let mut source_builder = ArtifactSource::new(self.name, source_path);

            if !self.includes.is_empty() {
                let source_includes = self.includes.iter().map(|s| s.to_string()).collect();
                source_builder = source_builder.with_includes(source_includes);
            }

            if !self.excludes.is_empty() {
                let source_excludes = self.excludes.iter().map(|s| s.to_string()).collect();
                source_builder = source_builder.with_excludes(source_excludes);
            }

            source_builder.build()
        };

        let work_dir = format!("./source/{}", source.name);

        // Resolve bun artifact
        let mut bun = Bun::new();
        if let Some(version) = &self.bun_version {
            bun = bun.with_version(version);
        }
        let bun = bun.build(context).await?;
        let bun_bin = format!("{}/bin", get_env_key(&bun));

        // Build source scripts string
        let source_scripts = if self.source_scripts.is_empty() {
            String::new()
        } else {
            format!("\n{}\n", self.source_scripts.join("\n"))
        };

        // Generate node modules symlink script (BTreeMap iterates in sorted key order)
        let node_modules_script = if self.node_modules.is_empty() {
            String::new()
        } else {
            let mut lines = vec!["mkdir -p node_modules".to_string()];
            for (package_name, digest) in &self.node_modules {
                let env_key = get_env_key(digest);
                if package_name.contains('/') {
                    // Scoped package like @vorpal/sdk
                    let scope = package_name.split('/').next().unwrap();
                    lines.push(format!("mkdir -p node_modules/{scope}"));
                    lines.push(format!("ln -sf {env_key} node_modules/{package_name}"));
                } else {
                    // Unscoped package like lodash
                    lines.push(format!("ln -sf {env_key} node_modules/{package_name}"));
                }
            }
            format!("\n{}", lines.join("\n"))
        };

        // Build command defaults to "bun run build"
        let build_command = self.build_command.unwrap_or("bun run build");

        let step_script = formatdoc! {r#"
            pushd "{work_dir}"
            {source_scripts}{node_modules_script}
            {bun_bin}/bun install --frozen-lockfile
            {bun_bin}/{build_command}

            mkdir -p "$VORPAL_OUTPUT"
            cp package.json "$VORPAL_OUTPUT/"
            cp -r dist "$VORPAL_OUTPUT/"
            cp -r node_modules "$VORPAL_OUTPUT/""#,
        };

        let mut step_environments = vec![format!("PATH={bun_bin}")];

        for env in self.environments {
            step_environments.push(env.to_string());
        }

        let mut step_artifacts = vec![bun.clone()];
        step_artifacts.extend(self.artifacts);

        for digest in self.node_modules.values() {
            step_artifacts.push(digest.clone());
        }

        let steps = vec![
            step::shell(
                context,
                step_artifacts,
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
