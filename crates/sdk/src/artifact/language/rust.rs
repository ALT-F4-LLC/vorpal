use crate::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{get_env_key, rust_toolchain, script, step, ArtifactBuilder, ArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use serde::Deserialize;
use std::{fs, path::Path};
use toml::from_str;

#[derive(Debug, Deserialize)]
struct RustArtifactCargoToml {
    bin: Option<Vec<RustArtifactCargoTomlBinary>>,
    workspace: Option<RustArtifactCargoTomlWorkspace>,
}

#[derive(Debug, Deserialize)]
struct RustArtifactCargoTomlBinary {
    name: String,
    path: String,
}

#[derive(Debug, Deserialize)]
struct RustArtifactCargoTomlWorkspace {
    members: Option<Vec<String>>,
}

fn read_cargo(path: &str) -> Result<RustArtifactCargoToml> {
    let contents = fs::read_to_string(path).expect("Failed to read Cargo.toml");

    Ok(from_str(&contents).expect("Failed to parse Cargo.toml"))
}

pub fn toolchain_target(system: ArtifactSystem) -> Result<String> {
    let target = match system {
        Aarch64Darwin => "aarch64-apple-darwin",
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        X8664Darwin => "x86_64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        _ => bail!("unsupported 'rust-toolchain' system: {:?}", system),
    };

    Ok(target.to_string())
}

pub fn toolchain_version() -> String {
    "1.83.0".to_string()
}

pub struct RustShellBuilder<'a> {
    artifacts: Vec<String>,
    name: &'a str,
}

pub struct RustBuilder<'a> {
    artifacts: Vec<String>,
    bins: Vec<String>,
    build: bool,
    check: bool,
    excludes: Vec<&'a str>,
    format: bool,
    lint: bool,
    name: &'a str,
    packages: Vec<String>,
    source: Option<String>,
    tests: bool,
}

impl<'a> RustShellBuilder<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            artifacts: vec![],
            name,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let mut artifacts = vec![];

        let toolchain = rust_toolchain::build(context).await?;
        let toolchain_target = toolchain_target(context.get_system())?;
        let toolchain_version = toolchain_version();

        artifacts.push(toolchain.clone());
        artifacts.extend(self.artifacts);

        let environments = vec![
            format!(
                "PATH={}/toolchains/{}-{}/bin",
                get_env_key(&toolchain),
                toolchain_version,
                toolchain_target
            ),
            format!("RUSTUP_HOME={}", get_env_key(&toolchain)),
            format!(
                "RUSTUP_TOOLCHAIN={}-{}",
                toolchain_version, toolchain_target
            ),
        ];

        // Create shell artifact

        script::devshell(context, artifacts, environments, self.name).await
    }
}

impl<'a> RustBuilder<'a> {
    pub fn new(name: &'a str) -> Self {
        Self {
            artifacts: vec![],
            bins: vec![],
            build: true,
            check: false,
            excludes: vec![],
            format: false,
            lint: false,
            name,
            packages: vec![],
            source: None,
            tests: false,
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

    pub fn with_lint(mut self, lint: bool) -> Self {
        self.lint = lint;
        self
    }

    pub fn with_packages(mut self, packages: Vec<&'a str>) -> Self {
        self.packages = packages.iter().map(|s| s.to_string()).collect();
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
        // 1. READ CARGO.TOML FILES

        // Get the source path

        let source_path = match self.source {
            Some(ref source) => Path::new(source),
            None => Path::new("."),
        };

        if !source_path.exists() {
            bail!(
                "Artifact `source.{}.path` not found: {:?}",
                self.name,
                source_path
            );
        }

        let source_path_str = source_path.display().to_string();

        // Load root cargo.toml

        let source_cargo_path = source_path.join("Cargo.toml");

        if !source_cargo_path.exists() {
            bail!("Cargo.toml not found: {:?}", source_cargo_path);
        }

        let source_cargo = read_cargo(source_cargo_path.to_str().unwrap())?;

        // TODO: implement for non-workspace based projects

        // Get list of bin targets

        let mut packages = vec![];
        let mut packages_bin_names = vec![];
        let mut packages_manifests = vec![];
        let mut packages_targets = vec![];

        if let Some(workspace) = source_cargo.workspace {
            if let Some(pkgs) = workspace.members {
                for package in pkgs {
                    if !self.packages.is_empty() && !self.packages.contains(&package) {
                        continue;
                    }

                    let package_path = source_path.join(package.clone());
                    let package_cargo_path = package_path.join("Cargo.toml");

                    if !package_cargo_path.exists() {
                        bail!("Cargo.toml not found: {:?}", package_cargo_path);
                    }

                    let package_cargo = read_cargo(package_cargo_path.to_str().unwrap())?;

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

                    for member_target_path in package_target_paths {
                        packages_targets.push(member_target_path);
                    }

                    packages.push(package);
                }
            }
        }

        // TODO: if no workspaces found then check source cargo

        // 2. CREATE ARTIFACTS

        // Get rust toolchain artifact

        let rust_toolchain = rust_toolchain::build(context).await?;
        let rust_toolchain_target = toolchain_target(context.get_system())?;
        let rust_toolchain_version = toolchain_version();
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

        let vendor_step_script = formatdoc! {r#"
            mkdir -pv $HOME

            pushd ./source/{name}-vendor

            cat > Cargo.toml << "EOF"
            [workspace]
            members = [{packages}]
            resolver = "2"
            EOF

            target_paths=({target_paths})

            for target_path in ${{target_paths[@]}}; do
                mkdir -pv $(dirname ${{target_path}})
                touch ${{target_path}}
            done

            mkdir -pv $VORPAL_OUTPUT/vendor

            cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

            echo "$cargo_vendor" > $VORPAL_OUTPUT/config.toml"#,
            name = self.name,
            packages = packages.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
            target_paths = packages_targets.iter().map(|s| format!("\"{}\"", s.display())).collect::<Vec<_>>().join(" "),
        };

        let vendor_step = step::shell(
            context,
            step_artifacts.clone(),
            step_environments.clone(),
            vendor_step_script,
        )
        .await?;

        let vendor_name = format!("{}-vendor", self.name);

        let vendor_source =
            ArtifactSourceBuilder::new(vendor_name.as_str(), source_path_str.as_str())
                .with_includes(vendor_cargo_paths.clone())
                .build();

        let vendor = ArtifactBuilder::new(vendor_name.as_str())
            .with_source(vendor_source)
            .with_step(vendor_step)
            .with_system(Aarch64Darwin)
            .with_system(Aarch64Linux)
            .with_system(X8664Darwin)
            .with_system(X8664Linux)
            .build(context)
            .await?;

        step_artifacts.push(vendor.clone());

        // Setup global values

        let mut source_includes = vec![];
        let mut source_excludes = vec!["target".to_string()];

        if !self.packages.is_empty() {
            for package in self.packages.into_iter() {
                source_includes.push(package);
            }
        }

        for exclude in self.excludes {
            source_excludes.push(exclude.to_string());
        }

        let mut source_builder = ArtifactSourceBuilder::new(self.name, source_path_str.as_str());

        if !source_includes.is_empty() {
            source_builder = source_builder.with_includes(source_includes);
        } else {
            source_builder = source_builder.with_excludes(source_excludes);
        }

        let source = source_builder.build();

        // Create artifact

        let step_script = formatdoc! {r#"
            mkdir -pv $HOME

            pushd ./source/{name}

            mkdir -pv .cargo
            mkdir -pv $VORPAL_OUTPUT/bin

            ln -sv {vendor}/config.toml .cargo/config.toml

            cat > Cargo.toml << "EOF"
            [workspace]
            members = [{packages}]
            resolver = "2"
            EOF

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
            name = self.name,
            packages = packages.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
            vendor = get_env_key(&vendor),
        };

        let step = step::shell(
            context,
            [step_artifacts.clone(), self.artifacts.clone()].concat(),
            step_environments,
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
