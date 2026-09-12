use crate::{
    api,
    api::artifact::ArtifactSystem,
    artifact::{
        bun::Bun, get_env_key, step, system, Artifact, ArtifactSource, DevelopmentEnvironment,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

/// Builds a `TypeScript`/JavaScript artifact (a compiled binary or a `bun`-installed package)
/// using the pinned `bun` runtime.
pub struct TypeScript<'a> {
    aliases: Vec<String>,
    artifacts: Vec<String>,
    entrypoint: Option<&'a str>,
    environments: Vec<&'a str>,
    name: &'a str,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    source_includes: Vec<&'a str>,
    source_scripts: Vec<String>,
    systems: Vec<ArtifactSystem>,
    system_error: Option<anyhow::Error>,
    working_dir: Option<String>,
}

impl<'a> TypeScript<'a> {
    /// Creates a builder for a `TypeScript`/JavaScript artifact named `name`, targeting
    /// `systems`.
    pub fn new<I, S>(name: &'a str, systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: system::ArtifactSystemInput,
    {
        let (systems, system_error) = system::normalize_systems_for_builder(systems);

        Self {
            aliases: vec![],
            artifacts: vec![],
            entrypoint: None,
            environments: vec![],
            name,
            secrets: vec![],
            source_includes: vec![],
            source_scripts: vec![],
            systems,
            system_error,
            working_dir: None,
        }
    }

    /// Adds aliases the built artifact will also be registered under, skipping duplicates.
    #[must_use]
    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        for alias in aliases {
            if !self.aliases.contains(&alias) {
                self.aliases.push(alias);
            }
        }
        self
    }

    /// Sets the dependency artifacts made available to the build step.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    /// Switches the build to compile a standalone binary via `bun build --compile` from this
    /// entrypoint, instead of running `tsc`.
    #[must_use]
    pub fn with_entrypoint(mut self, entrypoint: &'a str) -> Self {
        self.entrypoint = Some(entrypoint);
        self
    }

    /// Sets extra environment variables for the build step.
    #[must_use]
    pub fn with_environments(mut self, environments: Vec<&'a str>) -> Self {
        self.environments = environments;
        self
    }

    /// Restricts the registered source to these paths (default: the whole source directory).
    #[must_use]
    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.source_includes = includes;
        self
    }

    /// Adds build-step secrets, keyed by name, skipping names already present.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Vec<(String, String)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets
                    .push(api::artifact::ArtifactStepSecret { name, value });
            }
        }

        self
    }

    /// Adds shell script fragments to run before `bun install`, skipping duplicates.
    #[must_use]
    pub fn with_source_scripts(mut self, scripts: Vec<String>) -> Self {
        for script in scripts {
            if !self.source_scripts.contains(&script) {
                self.source_scripts.push(script);
            }
        }
        self
    }

    /// Runs the build from `dir`, relative to the registered source root.
    #[must_use]
    pub fn with_working_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    /// Builds the `TypeScript`/JavaScript artifact.
    ///
    /// # Errors
    ///
    /// Returns an error if a requested system string failed to parse, if building the `bun`
    /// toolchain dependency fails, or if registering the source or artifact with the build
    /// context fails.
    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        // Setup artifacts

        let bun = Bun::new().build(context).await?;
        let bun_bin = format!("{}/bin", get_env_key(&bun));

        // Setup source

        let source_path = ".";

        let mut source_builder = ArtifactSource::new(self.name, source_path);

        if !self.source_includes.is_empty() {
            source_builder = source_builder.with_includes(
                self.source_includes
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
            );
        }

        let source = source_builder.build();

        // Setup step source directory

        let step_source_dir = format!("{}/source/{}", source_path, source.name);

        let step_source_dir = match self.working_dir {
            Some(ref working_dir) => format!("{step_source_dir}/{working_dir}"),
            None => step_source_dir,
        };

        // Setup build command

        let step_build_command = match self.entrypoint {
            Some(entrypoint) => formatdoc! {r"
                mkdir -p $VORPAL_OUTPUT/bin

                {bun_bin}/bun build --compile {entrypoint} --outfile {name}

                cp {name} $VORPAL_OUTPUT/bin/{name}",
                name = self.name,
            },
            None => formatdoc! {r"
                mkdir -p $VORPAL_OUTPUT

                {bun_bin}/bun x tsc --project tsconfig.json --outDir dist

                cp package.json $VORPAL_OUTPUT/
                cp -r dist $VORPAL_OUTPUT/
                cp -r node_modules $VORPAL_OUTPUT/",
            },
        };

        // Build step script

        let step_script = formatdoc! {r"
            pushd {step_source_dir}

            {step_source_scripts}

            {bun_bin}/bun install --frozen-lockfile

            {step_build_command}",
            step_source_scripts = self.source_scripts.join("\n")
        };

        let mut step_environments = vec![format!("PATH={bun_bin}")];

        for env in self.environments {
            step_environments.push(env.to_string());
        }

        let mut step_artifacts = vec![bun];

        step_artifacts.extend(self.artifacts);

        // Sort for deterministic output

        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

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
            .with_aliases(self.aliases)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}

/// Development environment preloaded with the pinned `bun` runtime.
pub struct TypeScriptDevelopmentEnvironment<'a> {
    artifacts: Vec<String>,
    environments: Vec<String>,
    name: &'a str,
    secrets: Vec<(&'a str, &'a str)>,
    systems: Vec<ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

impl<'a> TypeScriptDevelopmentEnvironment<'a> {
    /// Creates a builder for a `TypeScript`/JavaScript development environment named `name`,
    /// targeting `systems`.
    pub fn new<I, S>(name: &'a str, systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: system::ArtifactSystemInput,
    {
        let (systems, system_error) = system::normalize_systems_for_builder(systems);

        Self {
            artifacts: vec![],
            environments: vec![],
            name,
            secrets: vec![],
            systems,
            system_error,
        }
    }

    /// Adds dependency artifacts made available in the environment.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts.extend(artifacts);
        self
    }

    /// Adds extra environment variables.
    #[must_use]
    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments.extend(environments);
        self
    }

    /// Adds environment secrets, keyed by name, skipping names already present.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Vec<(&'a str, &'a str)>) -> Self {
        for secret in secrets {
            if !self.secrets.iter().any(|(name, _)| *name == secret.0) {
                self.secrets.push(secret);
            }
        }
        self
    }

    /// Builds the `TypeScript`/JavaScript development environment artifact.
    ///
    /// # Errors
    ///
    /// Returns an error if a requested system string failed to parse, if building the `bun`
    /// toolchain dependency fails, or if registering the environment artifact with the build
    /// context fails.
    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        let bun = Bun::new().build(context).await?;

        let mut artifacts = vec![bun];
        artifacts.extend(self.artifacts);

        let mut devenv = DevelopmentEnvironment::new(self.name, self.systems)
            .with_artifacts(artifacts)
            .with_environments(self.environments);

        if !self.secrets.is_empty() {
            devenv = devenv.with_secrets(self.secrets);
        }

        devenv.build(context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Linux};

    #[test]
    fn constructors_accept_raw_string_arrays_and_enum_vectors() {
        let artifact = TypeScript::new("example", ["aarch64-darwin", "x86_64-linux"]);

        assert_eq!(artifact.systems, vec![Aarch64Darwin, X8664Linux]);
        assert!(artifact.system_error.is_none());

        let artifact = TypeScript::new("example", vec![Aarch64Linux, X8664Linux]);

        assert_eq!(artifact.systems, vec![Aarch64Linux, X8664Linux]);
        assert!(artifact.system_error.is_none());

        let devenv = TypeScriptDevelopmentEnvironment::new("example-dev", ["aarch64-darwin"]);

        assert_eq!(devenv.systems, vec![Aarch64Darwin]);
        assert!(devenv.system_error.is_none());
    }
}
