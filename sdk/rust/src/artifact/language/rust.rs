use crate::{
    api::artifact::{ArtifactStepSecret, ArtifactSystem},
    artifact::{get_env_key, protoc, rust_toolchain, step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use serde::Deserialize;
use std::fs::read_to_string;
use toml::from_str;

#[derive(Debug, Deserialize)]
struct RustArtifactCargoToml {
    bin: Option<Vec<RustArtifactCargoTomlBinary>>,
    package: Option<RustArtifactCargoTomlPackage>,
    workspace: Option<RustArtifactCargoTomlWorkspace>,
}

#[derive(Debug, Deserialize)]
struct RustArtifactCargoTomlBinary {
    name: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct RustArtifactCargoTomlPackage {
    name: String,
    // version: String,
}

#[derive(Debug, Deserialize)]
struct RustArtifactCargoTomlWorkspace {
    members: Option<Vec<String>>,
}

pub struct RustBuilder<'a> {
    artifacts: Vec<String>,
    bins: Vec<String>,
    build: bool,
    check: bool,
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
}

fn parse_cargo(path: &str) -> Result<RustArtifactCargoToml> {
    let contents = read_to_string(path).expect("Failed to read Cargo.toml");

    Ok(from_str(&contents).expect("Failed to parse Cargo.toml"))
}

impl<'a> RustBuilder<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            bins: vec![],
            build: true,
            check: false,
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
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_bins(mut self, bins: Vec<&str>) -> Self {
        self.bins = bins.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_check(mut self) -> Self {
        self.check = true;
        self
    }

    pub fn with_excludes(mut self, excludes: Vec<&'a str>) -> Self {
        self.excludes = excludes;
        self
    }

    pub fn with_format(mut self, format: bool) -> Self {
        self.format = format;
        self
    }

    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.includes = includes;
        self
    }

    pub fn with_lint(mut self, lint: bool) -> Self {
        self.lint = lint;
        self
    }

    pub fn with_packages(mut self, packages: Vec<&'a str>) -> Self {
        self.packages = packages.iter().map(|s| s.to_string()).collect();
        self
    }

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

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_tests(mut self, tests: bool) -> Self {
        self.tests = tests;
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let protoc = protoc::build(context).await?;

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
            bail!("Cargo.toml not found: {:?}", source_cargo_path);
        }

        let source_cargo = parse_cargo(source_cargo_path.to_str().unwrap())?;

        // Get list of bin targets

        let mut packages = vec![];
        let mut packages_bin_names = vec![];
        let mut packages_manifests = vec![];
        let mut packages_targets = vec![];

        if let Some(workspace) = source_cargo.workspace {
            if let Some(members) = workspace.members {
                for member in members {
                    let package_path = context_path_source.join(member.clone());
                    let package_cargo_path = package_path.join("Cargo.toml");

                    if !package_cargo_path.exists() {
                        bail!("Cargo.toml not found: {:?}", package_cargo_path);
                    }

                    let package_cargo = parse_cargo(package_cargo_path.to_str().unwrap())?;

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

                    for member_target_path in package_target_paths.iter() {
                        let member_target_path_relative = member_target_path
                            .strip_prefix(&context_path_source)
                            .unwrap_or(&member_target_path)
                            .to_path_buf();

                        packages_targets.push(member_target_path_relative);
                    }

                    packages.push(member);
                }
            }
        }

        // 2. CREATE ARTIFACTS

        // Get rust toolchain artifact

        let rust_toolchain = rust_toolchain::build(context).await?;
        let rust_toolchain_target = rust_toolchain::target(context.get_system())?;
        let rust_toolchain_version = rust_toolchain::version();
        let rust_toolchain_name = format!("{}-{}", rust_toolchain_version, rust_toolchain_target);

        // Set environment variables

        let mut step_artifacts = vec![rust_toolchain.clone()];

        let step_environments = vec![
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

        // Create vendor artifact

        let mut vendor_cargo_paths = vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()];

        for package in packages.iter() {
            vendor_cargo_paths.push(format!("{}/Cargo.toml", package));
        }

        let mut vendor_step_script = formatdoc! {r#"
            mkdir -pv $HOME

            pushd ./source/{name}-vendor"#,
            name = self.name,
        };

        if !packages.is_empty() {
            vendor_step_script = formatdoc! {r#"
                {vendor_step_script}

                cat > Cargo.toml << "EOF"
                [workspace]
                members = [{packages}]
                resolver = "2"
                EOF

                target_paths=({target_paths})

                for target_path in ${{target_paths[@]}}; do
                    mkdir -pv $(dirname ${{target_path}})
                    touch ${{target_path}}
                done"#,
                packages = packages.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                target_paths = packages_targets.iter().map(|s| format!("\"{}\"", s.display())).collect::<Vec<_>>().join(" "),
            };
        } else {
            vendor_step_script = formatdoc! {r#"
                {vendor_step_script}

                mkdir -pv src
                touch src/main.rs"#,
            };
        }

        vendor_step_script = formatdoc! {r#"
            {vendor_step_script}

            mkdir -pv $VORPAL_OUTPUT/vendor

            cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

            echo "$cargo_vendor" > $VORPAL_OUTPUT/config.toml"#,
        };

        let vendor_steps = vec![
            step::shell(
                context,
                step_artifacts.clone(),
                step_environments.clone(),
                vendor_step_script,
                self.secrets.clone(),
            )
            .await?,
        ];

        let vendor_name = format!("{}-vendor", self.name);

        let vendor_source = ArtifactSourceBuilder::new(vendor_name.as_str(), source_path)
            .with_includes(vendor_cargo_paths.clone())
            .build();

        let vendor = ArtifactBuilder::new(vendor_name.as_str(), vendor_steps, self.systems.clone())
            .with_sources(vec![vendor_source])
            .build(context)
            .await?;

        step_artifacts.push(vendor.clone());
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

        let source = ArtifactSourceBuilder::new(self.name, source_path)
            .with_includes(source_includes)
            .with_excludes(source_excludes)
            .build();

        // Create step

        let mut step_script = formatdoc! {r#"
            mkdir -pv $HOME

            pushd ./source/{name}

            mkdir -pv .cargo
            mkdir -pv $VORPAL_OUTPUT/bin

            ln -sv {vendor}/config.toml .cargo/config.toml"#,
            name = self.name,
            vendor = get_env_key(&vendor),
        };

        if !self.packages.is_empty() {
            step_script = formatdoc! {r#"
                {step_script}

                cat > Cargo.toml << "EOF"
                [workspace]
                members = [{packages}]
                resolver = "2"
                EOF"#,
                packages = packages.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
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

                cp -pv ./target/release/${{bin_name}} $VORPAL_OUTPUT/bin/
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
                [step_artifacts.clone(), self.artifacts.clone()].concat(),
                step_environments,
                step_script,
                self.secrets,
            )
            .await?,
        ];

        // Create artifact

        ArtifactBuilder::new(self.name, steps, self.systems)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}
