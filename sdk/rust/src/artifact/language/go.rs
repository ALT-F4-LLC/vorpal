use crate::{
    api,
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{get_env_key, git::Git, go::Go as GoDist, step, Artifact, ArtifactSource},
    context,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub struct Go<'a> {
    aliases: Vec<String>,
    artifacts: Vec<String>,
    build_directory: Option<&'a str>,
    build_flags: Option<&'a str>,
    build_path: Option<&'a str>,
    environments: Vec<&'a str>,
    includes: Vec<&'a str>,
    name: &'a str,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    source: Option<api::artifact::ArtifactSource>,
    source_scripts: Vec<String>,
    systems: Vec<ArtifactSystem>,
}

pub fn get_goos(target: ArtifactSystem) -> Result<String> {
    let goos = match target {
        Aarch64Darwin | X8664Darwin => "darwin",
        Aarch64Linux | X8664Linux => "linux",
        _ => bail!("unsupported 'go' system: {:?}", target),
    };

    Ok(goos.to_string())
}

pub fn get_goarch(target: ArtifactSystem) -> Result<String> {
    let goarch = match target {
        Aarch64Darwin | Aarch64Linux => "arm64",
        X8664Darwin | X8664Linux => "amd64",
        _ => bail!("unsupported 'go' system: {:?}", target),
    };

    Ok(goarch.to_string())
}

impl<'a> Go<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            aliases: vec![],
            artifacts: vec![],
            build_directory: None,
            build_flags: None,
            build_path: None,
            environments: vec![],
            includes: vec![],
            name,
            secrets: vec![],
            source: None,
            source_scripts: vec![],
            systems,
        }
    }

    pub fn with_alias(mut self, alias: String) -> Self {
        if !self.aliases.contains(&alias) {
            self.aliases.push(alias);
        }
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_build_directory(mut self, directory: &'a str) -> Self {
        self.build_directory = Some(directory);
        self
    }

    pub fn with_build_flags(mut self, flags: &'a str) -> Self {
        self.build_flags = Some(flags);
        self
    }

    pub fn with_build_path(mut self, path: &'a str) -> Self {
        self.build_path = Some(path);
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

    pub fn with_source_script(mut self, script: &'a str) -> Self {
        if !self.source_scripts.contains(&script.to_string()) {
            self.source_scripts.push(script.to_string());
        }
        self
    }

    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let source_path = ".";

        let mut source_builder = ArtifactSource::new(self.name, source_path);

        if !self.includes.is_empty() {
            let source_includes = self.includes.iter().map(|s| s.to_string()).collect();
            source_builder = source_builder.with_includes(source_includes);
        }

        let mut source = source_builder.build();

        if let Some(src) = self.source {
            source = src;
        }

        let source_dir = format!("./source/{}", source.name);

        let mut step_script = formatdoc! {r#"
            pushd {source_dir}

            mkdir -p $VORPAL_OUTPUT/bin"#,
        };

        if !self.source_scripts.is_empty() {
            let source_scripts = self.source_scripts.join("\n");

            step_script = formatdoc! {r#"
                {step_script}

                {source_scripts}"#,
            };
        }

        let build_directory = self.build_directory.unwrap_or(source_path);
        let build_flags = self.build_flags.unwrap_or("");
        let build_path = self.build_path.unwrap_or(source_path);

        step_script = formatdoc! {r#"
            {step_script}

            go build -C {build_directory} -o $VORPAL_OUTPUT/bin/{name} {build_flags} {build_path}

            go clean -modcache"#,
            name = self.name,
        };

        let git = Git::new().build(context).await?;
        let go = GoDist::new().build(context).await?;
        let goarch = get_goarch(context.get_system())?;
        let goos = get_goos(context.get_system())?;

        let mut step_environments = vec![
            format!("GOARCH={}", goarch),
            "GOCACHE=$VORPAL_WORKSPACE/go/cache".to_string(),
            format!("GOOS={}", goos),
            "GOPATH=$VORPAL_WORKSPACE/go".to_string(),
            format!("PATH={}/bin", get_env_key(&go)),
        ];

        for env in self.environments {
            step_environments.push(env.to_string());
        }

        let steps = vec![
            step::shell(
                context,
                [vec![git.clone(), go.clone()], self.artifacts].concat(),
                step_environments,
                step_script,
                self.secrets,
            )
            .await?,
        ];

        Artifact::new(self.name, steps, self.systems)
            .with_aliases(self.aliases)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
