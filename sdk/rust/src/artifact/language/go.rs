use crate::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{
        get_env_key, go, step, ArtifactBuilder, ArtifactSource, ArtifactSourceBuilder,
        ArtifactStepSecret,
    },
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub struct GoBuilder<'a> {
    aliases: Vec<String>,
    artifacts: Vec<String>,
    build_arguments: Vec<String>,
    build_directory: Option<&'a str>,
    build_path: Option<&'a str>,
    includes: Vec<&'a str>,
    name: &'a str,
    secrets: Vec<ArtifactStepSecret>,
    source: Option<ArtifactSource>,
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

impl<'a> GoBuilder<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            aliases: vec![],
            artifacts: vec![],
            build_arguments: vec![],
            build_directory: None,
            build_path: None,
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

    pub fn with_build_arguments(mut self, arguments: Vec<String>) -> Self {
        self.build_arguments = arguments;
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

    pub fn with_build_path(mut self, path: &'a str) -> Self {
        self.build_path = Some(path);
        self
    }

    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.includes = includes;
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(String, String)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(ArtifactStepSecret { name, value });
            }
        }
        self
    }

    pub fn with_source(mut self, source: ArtifactSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_source_script(mut self, script: &'a str) -> Self {
        if !self.source_scripts.contains(&script.to_string()) {
            self.source_scripts.push(script.to_string());
        }
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let source_path = ".";

        let mut source_builder = ArtifactSourceBuilder::new(self.name, source_path);

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
        let build_path = self.build_path.unwrap_or(source_path);

        step_script = formatdoc! {r#"
            {step_script}

            go build -C {build_directory} -o $VORPAL_OUTPUT/bin/{name} {build_path}

            go clean -modcache"#,
            name = self.name,
        };

        let go = go::build(context).await?;
        let goarch = get_goarch(context.get_system())?;
        let goos = get_goos(context.get_system())?;

        let steps = vec![
            step::shell(
                context,
                [vec![go.clone()], self.artifacts].concat(),
                vec![
                    "CGO_ENABLED=0".to_string(),
                    format!("GOARCH={}", goarch),
                    "GOCACHE=$VORPAL_WORKSPACE/go/cache".to_string(),
                    format!("GOOS={}", goos),
                    "GOPATH=$VORPAL_WORKSPACE/go".to_string(),
                    format!("PATH={}/bin", get_env_key(&go)),
                ],
                step_script,
                self.secrets,
            )
            .await?,
        ];

        ArtifactBuilder::new(self.name, steps, self.systems)
            .with_aliases(self.aliases)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
