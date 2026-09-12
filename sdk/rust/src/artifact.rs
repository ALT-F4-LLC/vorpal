use crate::{api, context};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Artifact for the Bun JavaScript runtime.
pub mod bun;
/// Artifact for the Cargo package manager.
pub mod cargo;
/// Artifact for the `cargo clippy` lint tool.
pub mod clippy;
/// Artifact for the `CPython` interpreter.
pub mod cpython;
/// Artifact for the `crane` OCI image tool.
pub mod crane;
/// Artifact for the GitHub CLI (`gh`).
pub mod gh;
/// Artifact for the Git version control tool.
pub mod git;
/// Artifact for the Go toolchain.
pub mod go;
/// Artifact for the `goimports` Go source formatter.
pub mod goimports;
/// Artifact for the `gopls` Go language server.
pub mod gopls;
/// Artifact for the `grpcurl` gRPC client tool.
pub mod grpcurl;
/// Language-specific artifact builders (Go, Python, Rust, TypeScript).
pub mod language;
/// Artifact for the Debian Linux base image.
pub mod linux_debian;
/// Artifact for the `linux-vorpal` sandbox root filesystem.
pub mod linux_vorpal;
/// Artifact for the slimmed-down `linux-vorpal` sandbox root filesystem.
pub mod linux_vorpal_slim;
/// Artifact for the Node.js JavaScript runtime.
pub mod nodejs;
/// Artifact for building OCI container images.
pub mod oci_image;
/// Artifact for the pnpm package manager.
pub mod pnpm;
/// Artifact for the Protocol Buffers compiler (`protoc`).
pub mod protoc;
/// Artifact for the `protoc-gen-go` Protocol Buffers plugin.
pub mod protoc_gen_go;
/// Artifact for the `protoc-gen-go-grpc` Protocol Buffers plugin.
pub mod protoc_gen_go_grpc;
/// Artifact for the `rsync` file synchronization tool.
pub mod rsync;
/// Artifact for the `rust-analyzer` Rust language server.
pub mod rust_analyzer;
/// Artifact for the Rust standard library source.
pub mod rust_src;
/// Artifact for the Rust standard library.
pub mod rust_std;
/// Artifact for the Rust toolchain.
pub mod rust_toolchain;
/// Artifact for the `rustc` Rust compiler.
pub mod rustc;
/// Artifact for the `rustfmt` Rust source formatter.
pub mod rustfmt;
/// Artifact for the `staticcheck` Go linter.
pub mod staticcheck;
/// Helpers for building the steps that make up an artifact.
pub mod step;
/// Target system parsing and normalization.
pub mod system;
/// Artifact for the `uv` Python package manager.
pub mod uv;

/// A named, optionally required build variable read from the config context.
pub struct Argument<'a> {
    /// Name of the build variable to read.
    pub name: &'a str,
    /// Whether [`Argument::build`] fails when the variable is unset.
    pub require: bool,
}

/// A named source directory or archive to include in an artifact, with
/// optional include/exclude filters and a pinned content digest.
pub struct ArtifactSource<'a> {
    /// Expected content digest of the source, when pinned.
    pub digest: Option<&'a str>,
    /// Glob patterns excluded from the source.
    pub excludes: Vec<String>,
    /// Glob patterns included from the source.
    pub includes: Vec<String>,
    /// Name identifying the source within the artifact.
    pub name: &'a str,
    /// Path to the source directory or archive, relative to the artifact
    /// context.
    pub path: &'a str,
}

/// One executable step of an artifact build, run under a specific
/// entrypoint (for example `bash` or `bwrap`).
pub struct ArtifactStep<'a> {
    /// Arguments passed to `entrypoint`.
    pub arguments: Vec<String>,
    /// Digests of artifacts made available to the step.
    pub artifacts: Vec<String>,
    /// Program that runs the step (for example `bash`, `bwrap`, `docker`).
    pub entrypoint: &'a str,
    /// Environment variables set for the step, as `key=value` strings.
    pub environments: Vec<String>,
    /// Secrets made available to the step.
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    /// Script content passed to `entrypoint`, when applicable.
    pub script: Option<String>,
}

/// Builder for a Vorpal artifact: its name, aliases, sources, build steps,
/// and supported target systems.
pub struct Artifact<'a> {
    /// Alternate names the artifact can be resolved by.
    pub aliases: Vec<String>,
    /// Name of the artifact.
    pub name: &'a str,
    /// Source directories or archives included in the artifact.
    pub sources: Vec<api::artifact::ArtifactSource>,
    /// Steps executed in order to build the artifact.
    pub steps: Vec<api::artifact::ArtifactStep>,
    systems: Vec<api::artifact::ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

/// Builder for an artifact that runs a single shell script step.
pub struct Job<'a> {
    /// Digests of artifacts made available to the job's script.
    pub artifacts: Vec<String>,
    /// Name of the resulting artifact.
    pub name: &'a str,
    /// Secrets made available to the job's script.
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    /// Shell script the job runs.
    pub script: String,
    systems: Vec<api::artifact::ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

/// Builder for an artifact that manages a long-running process, exposing
/// generated `start`, `stop`, and `logs` commands.
pub struct Process<'a> {
    /// Arguments passed to `entrypoint` when the process starts.
    pub arguments: Vec<String>,
    /// Digests of artifacts made available on `PATH` when the process runs.
    pub artifacts: Vec<String>,
    /// Program the process runs.
    pub entrypoint: &'a str,
    /// Name of the resulting artifact.
    pub name: &'a str,
    /// Secrets made available to the process.
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    systems: Vec<api::artifact::ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

/// Builder for an artifact that generates an activatable shell environment
/// exporting the given artifacts and environment variables.
pub struct DevelopmentEnvironment<'a> {
    /// Digests of artifacts made available on `PATH` in the environment.
    pub artifacts: Vec<String>,
    /// Environment variables exported by the environment, as `key=value`
    /// strings.
    pub environments: Vec<String>,
    /// Name of the resulting artifact.
    pub name: &'a str,
    /// Secrets made available in the environment.
    pub secrets: Vec<api::artifact::ArtifactStepSecret>,
    systems: Vec<api::artifact::ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

/// Builder for an artifact that installs a user-level environment: exported
/// variables plus symlinks activated into the user's home directory.
pub struct UserEnvironment<'a> {
    /// Digests of artifacts made available on `PATH` in the environment.
    pub artifacts: Vec<String>,
    /// Environment variables exported by the environment, as `key=value`
    /// strings.
    pub environments: Vec<String>,
    /// Name of the resulting artifact.
    pub name: &'a str,
    /// `(source, target)` symlink pairs activated into the user's home
    /// directory.
    pub symlinks: Vec<(String, String)>,
    systems: Vec<api::artifact::ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

impl<'a> Argument<'a> {
    /// Creates an optional build variable argument named `name`.
    #[must_use]
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            require: false,
        }
    }

    /// Makes [`Argument::build`] fail when the variable is unset.
    #[must_use]
    pub fn with_require(mut self) -> Self {
        self.require = true;
        self
    }

    /// Reads the build variable's value from `context`.
    ///
    /// # Errors
    ///
    /// Returns an error if the argument is required and the variable is
    /// unset.
    pub fn build(self, context: &mut context::ConfigContext) -> Result<Option<String>> {
        let variable = context.get_variable(self.name);

        if self.require && variable.is_none() {
            bail!("variable '{}' is required", self.name)
        }

        Ok(variable)
    }
}

impl<'a> ArtifactSource<'a> {
    /// Creates a source named `name` at `path`, with no filters or pinned
    /// digest.
    #[must_use]
    pub fn new(name: &'a str, path: &'a str) -> Self {
        Self {
            digest: None,
            excludes: vec![],
            includes: vec![],
            name,
            path,
        }
    }

    /// Pins the source to the given expected content digest.
    #[must_use]
    pub fn with_digest(mut self, digest: &'a str) -> Self {
        self.digest = Some(digest);
        self
    }

    /// Sets the glob patterns excluded from the source.
    #[must_use]
    pub fn with_excludes(mut self, excludes: Vec<String>) -> Self {
        self.excludes = excludes;
        self
    }

    /// Sets the glob patterns included from the source.
    #[must_use]
    pub fn with_includes(mut self, includes: Vec<String>) -> Self {
        self.includes = includes;
        self
    }

    /// Builds the protobuf `ArtifactSource` message.
    #[must_use]
    pub fn build(self) -> api::artifact::ArtifactSource {
        api::artifact::ArtifactSource {
            digest: self.digest.map(std::string::ToString::to_string),
            includes: self.includes,
            excludes: self.excludes,
            name: self.name.to_string(),
            path: self.path.to_string(),
        }
    }
}

impl<'a> ArtifactStep<'a> {
    /// Creates a step that runs the given `entrypoint` with no arguments,
    /// artifacts, environment, secrets, or script.
    #[must_use]
    pub fn new(entrypoint: &'a str) -> Self {
        Self {
            arguments: vec![],
            artifacts: vec![],
            entrypoint,
            environments: vec![],
            secrets: vec![],
            script: None,
        }
    }

    /// Sets the arguments passed to the step's entrypoint.
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "public SDK API; changing the signature is a breaking change"
    )]
    pub fn with_arguments(mut self, arguments: Vec<&str>) -> Self {
        self.arguments = arguments
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self
    }

    /// Sets the digests of artifacts made available to the step.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    /// Sets the environment variables set for the step.
    #[must_use]
    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments = environments;
        self
    }

    /// Adds secrets made available to the step, skipping any whose name is
    /// already set.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Vec<api::artifact::ArtifactStepSecret>) -> Self {
        for secret in secrets {
            if !self.secrets.iter().any(|s| s.name == secret.name) {
                self.secrets.push(secret);
            }
        }
        self
    }

    /// Sets the script content passed to the step's entrypoint.
    #[must_use]
    pub fn with_script(mut self, script: String) -> Self {
        self.script = Some(script);
        self
    }

    /// Builds the protobuf `ArtifactStep` message.
    #[must_use]
    pub fn build(self) -> api::artifact::ArtifactStep {
        api::artifact::ArtifactStep {
            arguments: self.arguments,
            artifacts: self.artifacts,
            entrypoint: Some(self.entrypoint.to_string()),
            environments: self.environments,
            secrets: self.secrets,
            script: self.script,
        }
    }
}

impl<'a> Artifact<'a> {
    /// Creates an artifact named `name` with the given build `steps`,
    /// supported for the given target `systems`. An unsupported system input
    /// is not rejected here; it surfaces as an error from
    /// [`Artifact::build`].
    pub fn new<I, S>(name: &'a str, steps: Vec<api::artifact::ArtifactStep>, systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: system::ArtifactSystemInput,
    {
        let (systems, system_error) = system::normalize_systems_for_builder(systems);

        Self {
            aliases: vec![],
            name,
            sources: vec![],
            steps,
            systems,
            system_error,
        }
    }

    /// Adds alternate names the artifact can be resolved by, skipping any
    /// already present.
    #[must_use]
    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        for alias in aliases {
            if !self.aliases.contains(&alias) {
                self.aliases.push(alias);
            }
        }
        self
    }

    /// Adds source directories or archives to the artifact, skipping any
    /// whose name is already present.
    #[must_use]
    pub fn with_sources(mut self, sources: Vec<api::artifact::ArtifactSource>) -> Self {
        for source in sources {
            if !self.sources.iter().any(|s| s.name == source.name) {
                self.sources.push(source);
            }
        }

        self
    }

    /// Resolves the artifact against `context`, sending it to the agent
    /// service to be prepared. Returns the digest of the prepared artifact.
    ///
    /// # Errors
    ///
    /// Returns an error if the artifact's target systems were invalid (see
    /// [`Artifact::new`]), or if resolving the artifact against `context`
    /// fails.
    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        let artifact = api::artifact::Artifact {
            aliases: self.aliases,
            name: self.name.to_string(),
            sources: self.sources,
            steps: self.steps,
            systems: self
                .systems
                .into_iter()
                .map(std::convert::Into::into)
                .collect(),
            target: context.get_system().into(),
        };

        context.add_artifact(&artifact).await
    }
}

impl<'a> Job<'a> {
    /// Creates a job named `name` that runs `script`, supported for the
    /// given target `systems`.
    pub fn new<I, S>(name: &'a str, script: String, systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: system::ArtifactSystemInput,
    {
        let (systems, system_error) = system::normalize_systems_for_builder(systems);

        Self {
            artifacts: vec![],
            name,
            secrets: vec![],
            script,
            systems,
            system_error,
        }
    }

    /// Sets the digests of artifacts made available to the job's script.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    /// Adds `(name, value)` secrets made available to the job's script,
    /// skipping any name already present.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Vec<(&str, &str)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(api::artifact::ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    /// Builds the job's shell step and resolves it as an artifact against
    /// `context`.
    ///
    /// # Errors
    ///
    /// Returns an error if the job's target systems were invalid (see
    /// [`Artifact::new`]), if building the shell step fails, or if resolving
    /// the resulting artifact against `context` fails.
    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let step = step::shell(context, self.artifacts, vec![], self.script, self.secrets).await?;

        Artifact::new(self.name, vec![step], self.systems)
            .build(context)
            .await
    }
}

impl<'a> DevelopmentEnvironment<'a> {
    /// Creates a development environment named `name`, supported for the
    /// given target `systems`.
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

    /// Sets the digests of artifacts made available on `PATH` in the
    /// environment.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    /// Sets the environment variables exported by the environment.
    #[must_use]
    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments = environments;
        self
    }

    /// Adds `(name, value)` secrets made available in the environment,
    /// skipping any name already present.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Vec<(&str, &str)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(api::artifact::ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    /// Builds the environment's activation script as a shell step and
    /// resolves it as an artifact against `context`.
    ///
    /// # Errors
    ///
    /// Returns an error if the environment's target systems were invalid
    /// (see [`Artifact::new`]), if building the shell step fails, or if
    /// resolving the resulting artifact against `context` fails.
    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let mut envs_backup = vec![
            "export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"".to_string(),
            "export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"".to_string(),
            "export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"".to_string(),
        ];

        let mut envs_export = vec![
            format!("export PS1=\"({}) $PS1\"", self.name),
            "export VORPAL_SHELL=\"1\"".to_string(),
        ];

        let mut envs_restore = vec![
            "export PATH=\"$VORPAL_SHELL_BACKUP_PATH\"".to_string(),
            "export PS1=\"$VORPAL_SHELL_BACKUP_PS1\"".to_string(),
            "export VORPAL_SHELL=\"$VORPAL_SHELL_BACKUP_VORPAL_SHELL\"".to_string(),
        ];

        let mut envs_unset = vec![
            "unset VORPAL_SHELL_BACKUP_PATH".to_string(),
            "unset VORPAL_SHELL_BACKUP_PS1".to_string(),
            "unset VORPAL_SHELL_BACKUP_VORPAL_SHELL".to_string(),
        ];

        for env in &self.environments {
            // `str::split` always yields at least one item, even for a
            // string with no '=' or an empty string.
            let key = env.split('=').next().unwrap_or_default();

            if key == "PATH" {
                continue;
            }

            envs_backup.push(format!("export VORPAL_SHELL_BACKUP_{key}=\"${key}\""));
            envs_export.push(format!("export {env}"));
            envs_restore.push(format!("export {key}=\"$VORPAL_SHELL_BACKUP_{key}\""));
            envs_unset.push(format!("unset VORPAL_SHELL_BACKUP_{key}"));
        }

        // Setup path

        let step_path_artifacts = self
            .artifacts
            .iter()
            .map(|artifact| format!("{}/bin", get_env_key(artifact)))
            .collect::<Vec<String>>()
            .join(":");

        let mut step_path = step_path_artifacts;

        if let Some(path) = self.environments.iter().find(|x| x.starts_with("PATH=")) {
            if let Some(path_value) = path.split('=').nth(1) {
                step_path = format!("{path_value}:{step_path}");
            }
        }

        envs_export.push(format!("export PATH={step_path}:$PATH"));

        // Setup script

        let step_script = formatdoc! {"
            mkdir -p $VORPAL_WORKSPACE/bin

            cat > bin/activate << \"EOF\"
            #!/bin/bash

            {backups}
            {exports}

            deactivate(){{
            {restores}
            {unsets}
            }}

            exec \"$@\"
            EOF

            chmod +x $VORPAL_WORKSPACE/bin/activate

            mkdir -p $VORPAL_OUTPUT/bin

            cp -pr bin \"$VORPAL_OUTPUT\"",
            backups = envs_backup.join("\n"),
            exports = envs_export.join("\n"),
            restores = envs_restore.join("\n"),
            unsets = envs_unset.join("\n"),
        };

        let steps =
            vec![step::shell(context, self.artifacts, vec![], step_script, self.secrets).await?];

        Artifact::new(self.name, steps, self.systems)
            .build(context)
            .await
    }
}

impl<'a> Process<'a> {
    /// Creates a process artifact named `name` that runs `entrypoint`,
    /// supported for the given target `systems`.
    pub fn new<I, S>(name: &'a str, entrypoint: &'a str, systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: system::ArtifactSystemInput,
    {
        let (systems, system_error) = system::normalize_systems_for_builder(systems);

        Self {
            arguments: vec![],
            artifacts: vec![],
            entrypoint,
            name,
            secrets: vec![],
            systems,
            system_error,
        }
    }

    /// Sets the arguments passed to `entrypoint` when the process starts.
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "public SDK API; changing the signature is a breaking change"
    )]
    pub fn with_arguments(mut self, arguments: Vec<&str>) -> Self {
        self.arguments = arguments
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self
    }

    /// Adds digests of artifacts made available on `PATH` when the process
    /// runs, skipping any already present.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        for artifact in artifacts {
            if !self.artifacts.contains(&artifact) {
                self.artifacts.push(artifact);
            }
        }
        self
    }

    /// Adds `(name, value)` secrets made available to the process, skipping
    /// any name already present.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Vec<(&str, &str)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(api::artifact::ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    /// Builds the process's `start`/`stop`/`logs` scripts as a shell step
    /// and resolves it as an artifact against `context`.
    ///
    /// # Errors
    ///
    /// Returns an error if the process's target systems were invalid (see
    /// [`Artifact::new`]), if building the shell step fails, or if resolving
    /// the resulting artifact against `context` fails.
    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let script = formatdoc! {r#"
            mkdir -p $VORPAL_OUTPUT/bin

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
                .arguments.clone()
                .join(" "),
            artifacts = self
                .artifacts
                .iter()
                .map(|v| format!("$VORPAL_ARTIFACT_{v}/bin"))
                .collect::<Vec<String>>()
                .join(":"),
            entrypoint = self.entrypoint,
            name = self.name,
        };

        let step = step::shell(context, self.artifacts, vec![], script, self.secrets).await?;

        Artifact::new(self.name, vec![step], self.systems)
            .build(context)
            .await
    }
}

impl<'a> UserEnvironment<'a> {
    /// Creates a user environment named `name`, supported for the given
    /// target `systems`.
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
            symlinks: vec![],
            systems,
            system_error,
        }
    }

    /// Sets the digests of artifacts made available on `PATH` in the
    /// environment.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    /// Sets the environment variables exported by the environment.
    #[must_use]
    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments = environments;
        self
    }

    /// Adds `(source, target)` symlink pairs activated into the user's home
    /// directory.
    #[must_use]
    pub fn with_symlinks(mut self, symlinks: Vec<(&str, &str)>) -> Self {
        for (source, target) in symlinks {
            self.symlinks.push((source.to_string(), target.to_string()));
        }
        self
    }

    /// Builds the environment's activation and symlink scripts as a shell
    /// step and resolves it as an artifact against `context`.
    ///
    /// # Errors
    ///
    /// Returns an error if the environment's target systems were invalid
    /// (see [`Artifact::new`]), if building the shell step fails, or if
    /// resolving the resulting artifact against `context` fails.
    pub async fn build(mut self, context: &mut context::ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        // Sort for deterministic output
        self.symlinks.sort_by(|a, b| a.0.cmp(&b.0));

        // Setup path

        let step_path_artifacts = self
            .artifacts
            .iter()
            .map(|artifact| format!("{}/bin", get_env_key(artifact)))
            .collect::<Vec<String>>()
            .join(":");

        let mut step_path = step_path_artifacts;

        if let Some(path) = self.environments.iter().find(|x| x.starts_with("PATH=")) {
            if let Some(path_value) = path.split('=').nth(1) {
                step_path = format!("{path_value}:{step_path}");
            }
        }

        // Setup script

        let step_script = formatdoc! {r#"
            mkdir -p $VORPAL_OUTPUT/bin

            cat > $VORPAL_OUTPUT/bin/vorpal-activate-shell << "EOF"
            {environments}
            export PATH="$VORPAL_OUTPUT/bin:{step_path}:$PATH"
            EOF

            cat > $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks << "EOF"
            #!/bin/bash
            set -euo pipefail
            {symlinks_deactivate}
            EOF

            cat > $VORPAL_OUTPUT/bin/vorpal-activate-symlinks << "EOF"
            #!/bin/bash
            set -euo pipefail
            {symlinks_check}
            {symlinks_activate}
            EOF

            cat > $VORPAL_OUTPUT/bin/vorpal-activate << "EOF"
            #!/bin/bash
            set -euo pipefail

            echo "Deactivating previous symlinks..."

            if [ -f $HOME/.vorpal/bin/vorpal-deactivate-symlinks ]; then
                $HOME/.vorpal/bin/vorpal-deactivate-symlinks
            fi

            echo "Activating symlinks..."

            $VORPAL_OUTPUT/bin/vorpal-activate-symlinks

            echo "Vorpal userenv installed. Run 'source vorpal-activate-shell' to activate."

            ln -sf $VORPAL_OUTPUT/bin/vorpal-activate-shell $HOME/.vorpal/bin/vorpal-activate-shell
            ln -sf $VORPAL_OUTPUT/bin/vorpal-activate-symlinks $HOME/.vorpal/bin/vorpal-activate-symlinks
            ln -sf $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks $HOME/.vorpal/bin/vorpal-deactivate-symlinks
            EOF


            chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-shell
            chmod +x $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks
            chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-symlinks
            chmod +x $VORPAL_OUTPUT/bin/vorpal-activate"#,
            environments = self.environments
                .iter()
                .filter(|e| !e.starts_with("PATH="))
                .map(|e| format!("export {e}"))
                .collect::<Vec<String>>()
                .join("\n"),
            symlinks_deactivate = self.symlinks
                .iter()
                .map(|(_, target)| format!("rm -f {target}"))
                .collect::<Vec<String>>()
                .join("\n"),
            symlinks_check = self.symlinks
                .iter()
                .map(|(_, target)| format!("if [ -f {target} ]; then echo \"ERROR: Symlink target exists -> {target}\" && exit 1; fi"))
                .collect::<Vec<String>>()
                .join("\n"),
            symlinks_activate = self.symlinks
                .iter()
                .map(|(source, target)| format!("ln -s {source} {target}"))
                .collect::<Vec<String>>()
                .join("\n"),
        };

        let steps = vec![step::shell(context, self.artifacts, vec![], step_script, vec![]).await?];

        Artifact::new(self.name, steps, self.systems)
            .build(context)
            .await
    }
}

/// Returns the default agent/registry socket address: the `VORPAL_SOCKET_PATH`
/// environment variable as a `unix://` URI when set and non-empty, otherwise
/// the standard Vorpal socket path.
#[must_use]
pub fn get_default_address() -> String {
    if let Ok(path) = std::env::var("VORPAL_SOCKET_PATH") {
        if !path.is_empty() {
            return format!("unix://{path}");
        }
    }
    "unix:///var/lib/vorpal/vorpal.sock".to_string()
}

/// Returns the shell variable reference (`$VORPAL_ARTIFACT_<digest>`) an
/// artifact's output directory is exposed under in a build step.
#[must_use]
pub fn get_env_key(digest: &String) -> String {
    format!("$VORPAL_ARTIFACT_{digest}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        agent::agent_service_client::AgentServiceClient,
        artifact::{
            artifact_service_client::ArtifactServiceClient,
            ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Linux},
        },
    };
    use std::path::PathBuf;
    use tonic::transport::Endpoint;

    fn test_context() -> Result<context::ConfigContext> {
        let channel = Endpoint::from_static("http://127.0.0.1:1").connect_lazy();

        context::ConfigContext::new(
            "test".to_string(),
            PathBuf::from("."),
            "library".to_string(),
            "aarch64-darwin".to_string(),
            false,
            vec![],
            AgentServiceClient::new(channel.clone()),
            ArtifactServiceClient::new(channel),
            0,
            "http://127.0.0.1:1".to_string(),
        )
    }

    #[test]
    fn artifact_new_accepts_raw_string_arrays_and_enum_vectors() {
        let from_strings = Artifact::new("example", vec![], ["aarch64-darwin", "x86_64-linux"]);

        assert_eq!(from_strings.systems, vec![Aarch64Darwin, X8664Linux]);
        assert!(from_strings.system_error.is_none());

        let from_enums = Artifact::new("example", vec![], vec![Aarch64Linux, X8664Linux]);

        assert_eq!(from_enums.systems, vec![Aarch64Linux, X8664Linux]);
        assert!(from_enums.system_error.is_none());
    }

    #[test]
    fn artifact_build_returns_stored_system_error() -> Result<()> {
        let runtime = tokio::runtime::Runtime::new()?;

        let result = runtime.block_on(async {
            let mut context = test_context()?;

            Artifact::new("example", vec![], ["loongarch64-linux"])
                .build(&mut context)
                .await
        });

        let Err(err) = result else {
            bail!("expected loongarch64-linux to be rejected");
        };

        assert_eq!(err.to_string(), "unsupported system: loongarch64-linux");

        Ok(())
    }
}
