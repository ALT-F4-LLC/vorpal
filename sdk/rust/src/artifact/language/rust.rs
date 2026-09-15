use crate::{
    api::artifact::{ArtifactStepSecret, ArtifactSystem},
    artifact::{
        get_env_key, protoc::Protoc, rust_toolchain, rust_toolchain::RustToolchain, step, system,
        Artifact, ArtifactSource, DevelopmentEnvironment,
    },
    context::ConfigContext,
};
use anyhow::{bail, Context, Result};
use indoc::formatdoc;
use serde::Deserialize;
use std::fs::read_to_string;
use std::path::Path;
use toml::from_str;

#[derive(Debug, Deserialize)]
struct RustCargoToml {
    bin: Option<Vec<RustCargoTomlBinary>>,
    package: Option<RustCargoTomlPackage>,
    workspace: Option<RustCargoTomlWorkspace>,
}

#[derive(Debug, Deserialize)]
struct RustCargoTomlBinary {
    name: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct RustCargoTomlPackage {
    name: String,
    // version: String,
}

#[derive(Debug, Deserialize)]
struct RustCargoTomlWorkspace {
    members: Option<Vec<String>>,
}

/// Builds a Rust binary artifact from a `Cargo.toml`/workspace, using a vendored-dependency
/// step for hermetic, network-free `cargo` invocations.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Rust is a builder; each bool is set independently via a named with_* method, so \
              no call site risks transposed positional bools; converting to a flags type is a \
              breaking SDK change out of scope here"
)]
pub struct Rust<'a> {
    artifacts: Vec<String>,
    bins: Vec<String>,
    build: bool,
    check: bool,
    environments: Vec<&'a str>,
    excludes: Vec<&'a str>,
    format: bool,
    includes: Vec<&'a str>,
    lint: bool,
    name: &'a str,
    packages: Vec<String>,
    secrets: Vec<ArtifactStepSecret>,
    source: Option<String>,
    tests: bool,
    systems: Vec<ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

/// Reads and parses a `Cargo.toml` manifest.
///
/// # Errors
///
/// Returns an error if `path` cannot be read or its contents are not valid `Cargo.toml` TOML.
fn parse_cargo(path: &Path) -> Result<RustCargoToml> {
    let contents =
        read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

impl<'a> Rust<'a> {
    /// Creates a builder for a Rust binary artifact named `name`, targeting `systems`.
    pub fn new<I, S>(name: &'a str, systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: system::ArtifactSystemInput,
    {
        let (systems, system_error) = system::normalize_systems_for_builder(systems);

        Self {
            artifacts: vec![],
            bins: vec![],
            build: true,
            check: false,
            environments: vec![],
            excludes: vec![],
            format: false,
            includes: vec![],
            lint: false,
            name,
            packages: vec![],
            secrets: vec![],
            source: None,
            tests: false,
            systems,
            system_error,
        }
    }

    /// Sets the dependency artifacts made available to the build step.
    #[must_use]
    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    /// Restricts which binary targets are built (default: every bin target in the selected
    /// packages).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "public SDK API; changing the signature is a breaking change"
    )]
    #[must_use]
    pub fn with_bins(mut self, bins: Vec<&str>) -> Self {
        self.bins = bins.iter().map(std::string::ToString::to_string).collect();
        self
    }

    /// Enables `cargo check --release` for each selected binary.
    #[must_use]
    pub fn with_check(mut self, check: bool) -> Self {
        self.check = check;
        self
    }

    /// Sets extra environment variables for the build step.
    #[must_use]
    pub fn with_environments(mut self, environments: Vec<&'a str>) -> Self {
        self.environments = environments;
        self
    }

    /// Excludes these paths from the registered source, in addition to `target`.
    #[must_use]
    pub fn with_excludes(mut self, excludes: Vec<&'a str>) -> Self {
        self.excludes = excludes;
        self
    }

    /// Enables `cargo fmt --all --check` before building.
    #[must_use]
    pub fn with_format(mut self, format: bool) -> Self {
        self.format = format;
        self
    }

    /// Restricts the registered source to these paths, in addition to the always-included
    /// `Cargo.toml`/`Cargo.lock` manifests (default: the whole source directory).
    #[must_use]
    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.includes = includes;
        self
    }

    /// Enables `cargo clippy --deny warnings` for each selected package manifest.
    #[must_use]
    pub fn with_lint(mut self, lint: bool) -> Self {
        self.lint = lint;
        self
    }

    /// Restricts which workspace packages are built (default: every workspace member).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "public SDK API; changing the signature is a breaking change"
    )]
    #[must_use]
    pub fn with_packages(mut self, packages: Vec<&'a str>) -> Self {
        self.packages = packages
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        self
    }

    /// Adds build-step secrets, keyed by name, skipping names already present.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Vec<(&str, &str)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets.push(ArtifactStepSecret {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }

        self
    }

    /// Overrides the source directory, relative to the config context (default: `.`).
    #[must_use]
    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    /// Enables `cargo test --release` for each selected binary.
    #[must_use]
    pub fn with_tests(mut self, tests: bool) -> Self {
        self.tests = tests;
        self
    }

    /// Builds the Rust binary artifact.
    ///
    /// # Errors
    ///
    /// Returns an error if a requested system string failed to parse, if the source directory
    /// or its `Cargo.toml` is missing or fails to parse, if building the `protoc` or Rust
    /// toolchain dependency fails, or if registering the vendor source, main source, or
    /// artifact with the build context fails.
    #[expect(
        clippy::too_many_lines,
        reason = "linear pipeline: parse Cargo.toml, resolve workspace bin targets, vendor \
                  dependencies, then assemble the build step script; splitting would scatter \
                  state shared across every stage"
    )]
    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        // Sort for deterministic output
        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let protoc = Protoc::new().build(context).await?;

        // Parse source path

        let context_path = context.get_artifact_context_path();

        let source_path = match self.source {
            Some(ref source) => source,
            None => ".",
        };

        let context_path_source = context_path.join(source_path);

        if !context_path_source.exists() {
            bail!("`source.{}.path` not found: {}", self.name, source_path);
        }

        // Parse cargo.toml

        let source_cargo_path = context_path_source.join("Cargo.toml");

        if !source_cargo_path.exists() {
            bail!("Cargo.toml not found: {}", source_cargo_path.display());
        }

        let source_cargo = parse_cargo(&source_cargo_path)?;

        // Get list of bin targets

        let mut packages = vec![];
        let mut packages_bin_names = vec![];
        let mut packages_manifests = vec![];
        let mut packages_targets = vec![];

        if let Some(workspace) = source_cargo.workspace {
            if let Some(members) = workspace.members {
                for member in members {
                    let package_path = context_path_source.join(&member);
                    let package_cargo_path = package_path.join("Cargo.toml");

                    if !package_cargo_path.exists() {
                        bail!("Cargo.toml not found: {}", package_cargo_path.display());
                    }

                    let package_cargo = parse_cargo(&package_cargo_path)?;

                    if !self.packages.is_empty() {
                        if let Some(ref package) = package_cargo.package {
                            if !self.packages.contains(&package.name) {
                                continue;
                            }
                        }
                    }

                    let mut package_target_paths = vec![];

                    if let Some(bins) = package_cargo.bin {
                        for bin in bins {
                            package_target_paths.push(package_path.join(bin.path));

                            if self.bins.is_empty() || self.bins.contains(&bin.name) {
                                let manifest_path = package_cargo_path.display().to_string();

                                if !packages_manifests.contains(&manifest_path) {
                                    packages_manifests.push(manifest_path);
                                }

                                packages_bin_names.push(bin.name);
                            }
                        }
                    }

                    if package_target_paths.is_empty() {
                        package_target_paths.push(package_path.join("src/lib.rs"));
                    }

                    for member_target_path in &package_target_paths {
                        let member_target_path_relative = member_target_path
                            .strip_prefix(&context_path_source)
                            .unwrap_or(member_target_path)
                            .to_path_buf();

                        packages_targets.push(member_target_path_relative);
                    }

                    packages.push(member);
                }
            }
        }

        // 2. CREATE ARTIFACTS

        // Get rust toolchain artifact

        let rust_toolchain = RustToolchain::new().build(context).await?;
        let rust_toolchain_target = rust_toolchain::target(context.get_system())?;
        let rust_toolchain_version = rust_toolchain::version();
        let rust_toolchain_name = format!("{rust_toolchain_version}-{rust_toolchain_target}");

        // Set environment variables

        let mut step_environments = vec![
            "HOME=$VORPAL_WORKSPACE/home".to_string(),
            format!(
                "PATH={}",
                format!(
                    "{}/toolchains/{}/bin",
                    get_env_key(&rust_toolchain),
                    rust_toolchain_name
                )
            ),
            format!("RUSTUP_HOME={}", get_env_key(&rust_toolchain)),
            format!("RUSTUP_TOOLCHAIN={}", rust_toolchain_name),
        ];

        let mut step_artifacts = vec![rust_toolchain];

        for environment in self.environments {
            step_environments.push(environment.to_string());
        }

        // Create vendor artifact

        let mut vendor_cargo_paths = vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()];

        for package in &packages {
            vendor_cargo_paths.push(format!("{package}/Cargo.toml"));
        }

        let mut vendor_step_script = formatdoc! {r"
            mkdir -p $HOME

            pushd ./source/{name}-vendor",
            name = self.name,
        };

        if packages.is_empty() {
            vendor_step_script = formatdoc! {r"
                {vendor_step_script}

                mkdir -p src
                touch src/main.rs",
            };
        } else {
            vendor_step_script = formatdoc! {r#"
                {vendor_step_script}

                cat > Cargo.toml << "EOF"
                [workspace]
                members = [{packages}]
                resolver = "2"

                [workspace.lints.rust]

                [workspace.lints.clippy]
                EOF

                target_paths=({target_paths})

                for target_path in ${{target_paths[@]}}; do
                    mkdir -p $(dirname ${{target_path}})
                    touch ${{target_path}}
                done"#,
                packages = packages.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(","),
                target_paths = packages_targets.iter().map(|s| format!("\"{}\"", s.display())).collect::<Vec<_>>().join(" "),
            };
        }

        vendor_step_script = formatdoc! {r#"
            {vendor_step_script}

            mkdir -p $VORPAL_OUTPUT/vendor

            cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

            echo "$cargo_vendor" > $VORPAL_OUTPUT/config.toml"#,
        };

        let vendor_steps = vec![
            step::shell(
                context,
                &step_artifacts,
                &step_environments,
                vendor_step_script,
                &self.secrets,
            )
            .await?,
        ];

        let vendor_name = format!("{}-vendor", self.name);

        let vendor_source = ArtifactSource::new(vendor_name.as_str(), source_path)
            .with_includes(vendor_cargo_paths)
            .build();

        let vendor = Artifact::new(vendor_name.as_str(), vendor_steps, &self.systems)
            .with_sources(vec![vendor_source])
            .build(context)
            .await?;

        let vendor_env_key = get_env_key(&vendor);

        step_artifacts.push(vendor);
        step_artifacts.push(protoc);

        // Create source

        let mut source_includes = vec![];
        let mut source_excludes = vec!["target".to_string()];

        for exclude in self.excludes {
            source_excludes.push(exclude.to_string());
        }

        for include in self.includes {
            source_includes.push(include.to_string());
        }

        let source = ArtifactSource::new(self.name, source_path)
            .with_includes(source_includes)
            .with_excludes(source_excludes)
            .build();

        // Create step

        let mut step_script = formatdoc! {r"
            mkdir -p $HOME

            pushd ./source/{name}

            mkdir -p .cargo
            mkdir -p $VORPAL_OUTPUT/bin

            ln -s {vendor}/config.toml .cargo/config.toml",
            name = self.name,
            vendor = vendor_env_key,
        };

        if !self.packages.is_empty() {
            step_script = formatdoc! {r#"
                {step_script}

                cat > Cargo.toml << "EOF"
                [workspace]
                members = [{packages}]
                resolver = "2"

                [workspace.lints.rust]

                [workspace.lints.clippy]
                EOF"#,
                packages = packages.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(","),
            };
        }

        if packages_bin_names.is_empty() {
            packages_bin_names.push(self.name.to_string());
        }

        if packages_manifests.is_empty() {
            packages_manifests.push(source_cargo_path.display().to_string());
        }

        step_script = formatdoc! {r#"
            {step_script}

            bin_names=({bin_names})
            manifest_paths=({manifest_paths})

            if [ "{enable_format}" = "true" ]; then
                echo "Running formatter..."
                cargo --offline fmt --all --check
            fi

            for manifest_path in ${{manifest_paths[@]}}; do
                if [ "{enable_lint}" = "true" ]; then
                    echo "Running linter..."
                    cargo --offline clippy --manifest-path ${{manifest_path}} -- --deny warnings
                fi
            done

            for bin_name in ${{bin_names[@]}}; do
                if [ "{enable_check}" = "true" ]; then
                    echo "Running check..."
                    cargo --offline check --bin ${{bin_name}} --release
                fi

                if [ "{enable_build}" = "true" ]; then
                    echo "Running build..."
                    cargo --offline build --bin ${{bin_name}} --release
                fi

                if [ "{enable_tests}" = "true" ]; then
                    echo "Running tests..."
                    cargo --offline test --bin ${{bin_name}} --release
                fi

                cp -p ./target/release/${{bin_name}} $VORPAL_OUTPUT/bin/
            done"#,
            bin_names = packages_bin_names.join(" "),
            enable_build = if self.build { "true" } else { "false" },
            enable_check = if self.check { "true" } else { "false" },
            enable_format = if self.format { "true" } else { "false" },
            enable_lint = if self.lint { "true" } else { "false" },
            enable_tests = if self.tests { "true" } else { "false" },
            manifest_paths = packages_manifests.join(" "),
        };

        let steps = vec![
            step::shell(
                context,
                &[step_artifacts, self.artifacts].concat(),
                &step_environments,
                step_script,
                &self.secrets,
            )
            .await?,
        ];

        // Create artifact

        Artifact::new(self.name, steps, self.systems)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}

// ---------------------------------------------------------------------------
// Rust Development Environment
// ---------------------------------------------------------------------------

/// Development environment preloaded with the Rust toolchain (rustc, cargo, clippy, rustfmt,
/// rust-src, rust-analyzer) and, by default, `protoc`.
pub struct RustDevelopmentEnvironment<'a> {
    artifacts: Vec<String>,
    environments: Vec<String>,
    include_protoc: bool,
    name: &'a str,
    secrets: Vec<(&'a str, &'a str)>,
    systems: Vec<ArtifactSystem>,
    system_error: Option<anyhow::Error>,
}

impl<'a> RustDevelopmentEnvironment<'a> {
    /// Creates a builder for a Rust development environment named `name`, targeting `systems`.
    pub fn new<I, S>(name: &'a str, systems: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: system::ArtifactSystemInput,
    {
        let (systems, system_error) = system::normalize_systems_for_builder(systems);

        Self {
            artifacts: vec![],
            environments: vec![],
            include_protoc: true,
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

    /// Excludes `protoc` from the environment.
    #[must_use]
    pub fn without_protoc(mut self) -> Self {
        self.include_protoc = false;
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

    /// Builds the Rust development environment artifact.
    ///
    /// # Errors
    ///
    /// Returns an error if a requested system string failed to parse, if building the Rust
    /// toolchain dependency or, unless excluded via [`Self::without_protoc`], the `protoc`
    /// dependency fails, or if registering the environment artifact with the build context
    /// fails.
    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        system::check_system_error(&mut self.system_error)?;

        let rust_toolchain_digest = RustToolchain::new().build(context).await?;

        let mut artifacts = vec![];

        if self.include_protoc {
            let protoc = Protoc::new().build(context).await?;
            artifacts.push(protoc);
        }

        let toolchain_target = rust_toolchain::target(context.get_system())?;
        let toolchain_version = rust_toolchain::version();
        let toolchain_name = format!("{toolchain_version}-{toolchain_target}");
        let toolchain_bin = format!(
            "{}/toolchains/{toolchain_name}/bin",
            get_env_key(&rust_toolchain_digest)
        );

        let mut environments = vec![
            format!("PATH={toolchain_bin}"),
            format!("RUSTUP_HOME={}", get_env_key(&rust_toolchain_digest)),
            format!("RUSTUP_TOOLCHAIN={toolchain_name}"),
        ];

        environments.extend(self.environments);

        artifacts.push(rust_toolchain_digest);
        artifacts.extend(self.artifacts);

        let mut devenv = DevelopmentEnvironment::new(self.name, self.systems)
            .with_artifacts(artifacts)
            .with_environments(environments);

        if !self.secrets.is_empty() {
            devenv = devenv.with_secrets(self.secrets);
        }

        devenv.build(context).await
    }
}
