use crate::{
    artifact::{get_env_key, go, step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::artifact::v0::{
    ArtifactSource, ArtifactSystem,
    ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};

pub struct GoBuilder<'a> {
    artifacts: Vec<String>,
    build_dir: Option<&'a str>,
    build_path: Option<&'a str>,
    includes: Vec<&'a str>,
    name: &'a str,
    source: Option<ArtifactSource>,
}

pub fn get_goos(target: ArtifactSystem) -> String {
    let goos = match target {
        Aarch64Darwin | X8664Darwin => "darwin",
        Aarch64Linux | X8664Linux => "linux",
        _ => unreachable!(),
    };

    goos.to_string()
}

pub fn get_goarch(target: ArtifactSystem) -> String {
    let goarch = match target {
        Aarch64Darwin | Aarch64Linux => "arm64",
        X8664Darwin => "amd64",
        X8664Linux => "386",
        _ => unreachable!(),
    };

    goarch.to_string()
}

impl<'a> GoBuilder<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            artifacts: vec![],
            build_dir: None,
            build_path: None,
            includes: vec![],
            name,
            source: None,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_build_dir(mut self, build_dir: &'a str) -> Self {
        self.build_dir = Some(build_dir);
        self
    }

    pub fn with_build_path(mut self, build_path: &'a str) -> Self {
        self.build_path = Some(build_path);
        self
    }

    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.includes = includes;
        self
    }

    pub fn with_source(mut self, source: ArtifactSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_artifact(mut self, artifact: String) -> Self {
        self.artifacts.push(artifact);
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let go = go::build(context).await?;

        let mut build_dir = format!("./source/{}", self.name);

        let source_path = ".";

        let mut source_builder = ArtifactSourceBuilder::new(self.name, source_path);

        if !self.includes.is_empty() {
            source_builder =
                source_builder.with_includes(self.includes.iter().map(|s| s.to_string()).collect());
        }

        let mut source = source_builder.build();

        if let Some(src) = self.source {
            source = src;
        }

        if let Some(dir) = self.build_dir {
            build_dir = format!("{}/{}", build_dir, dir);
        }

        let build_path = self.build_path.unwrap_or(source_path);

        let step_script = formatdoc! {"
            pushd {build_dir}

            mkdir -p $VORPAL_OUTPUT/bin

            go build -o $VORPAL_OUTPUT/bin/{name} {build_path}

            go clean -modcache",
            name = self.name,
        };

        let step = step::shell(
            context,
            vec![vec![go.clone()], self.artifacts].concat(),
            vec![
                "CGO_ENABLED=0".to_string(),
                format!("GOARCH={}", get_goarch(context.get_target())),
                "GOCACHE=$VORPAL_WORKSPACE/go/cache".to_string(),
                format!("GOOS={}", get_goos(context.get_target())),
                "GOPATH=$VORPAL_WORKSPACE/go".to_string(),
                format!("PATH={}/bin", get_env_key(&go)),
            ],
            step_script,
        )
        .await?;

        ArtifactBuilder::new(self.name)
            .with_source(source)
            .with_step(step)
            .with_system(Aarch64Darwin)
            .with_system(Aarch64Linux)
            .with_system(X8664Darwin)
            .with_system(X8664Linux)
            .build(context)
            .await
    }
}
