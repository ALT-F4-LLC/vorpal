use crate::{
    artifact::{get_env_key, shell, step, ConfigArtifactBuilder, ConfigArtifactSourceBuilder},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use toml::from_str;
use vorpal_schema::config::v0::{
    ConfigArtifactSystem,
    ConfigArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};

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

pub fn toolchain_hash(target: ConfigArtifactSystem) -> Result<&'static str> {
    let target = match target {
        Aarch64Darwin => "79d82dbb5e0db73b239859a74f070a6135adff3e1a13d05aa3dcb6863d819a70",
        Aarch64Linux => "",
        X8664Darwin => "",
        X8664Linux => "",
        _ => bail!("unsupported 'rust-toolchain' target: {:?}", target),
    };

    Ok(target)
}

pub fn toolchain_target(target: ConfigArtifactSystem) -> Result<String> {
    let target = match target {
        Aarch64Darwin => "aarch64-apple-darwin",
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        X8664Darwin => "x86_64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        _ => bail!("unsupported 'rust-toolchain' target: {:?}", target),
    };

    Ok(target.to_string())
}

pub fn toolchain_version() -> String {
    "1.83.0".to_string()
}

pub async fn devshell(
    context: &mut ConfigContext,
    artifacts: Vec<String>,
    name: &str,
) -> Result<String> {
    let mut devshell_artifacts = vec![];

    let toolchain = context
        .fetch_artifact(toolchain_hash(context.get_target())?)
        .await?;
    let toolchain_target = toolchain_target(context.get_target())?;
    let toolchain_version = toolchain_version();

    devshell_artifacts.push(toolchain.clone());

    devshell_artifacts.extend(artifacts);

    let devshell_environments = vec![
        format!(
            "PATH={}/toolchains/{}-{}/bin:$PATH",
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

    shell::build(context, devshell_artifacts, devshell_environments, name).await
}

pub async fn package<'a>(
    context: &mut ConfigContext,
    artifacts: Vec<String>,
    name: &'a str,
    excludes: Vec<&'a str>,
) -> Result<String> {
    // 1. READ CARGO.TOML FILES

    // Get the source path
    let source_path = Path::new(".").to_path_buf();

    if !source_path.exists() {
        bail!(
            "Artifact `source.{}.path` not found: {:?}",
            name,
            source_path
        );
    }

    let source_path_str = source_path.display().to_string();

    // Load root cargo.toml

    let cargo_toml_path = source_path.join("Cargo.toml");

    if !cargo_toml_path.exists() {
        bail!("Cargo.toml not found: {:?}", cargo_toml_path);
    }

    let cargo_toml = read_cargo(cargo_toml_path.to_str().unwrap())?;

    // TODO: implement for non-workspace based projects

    // Get list of bin targets

    let mut workspaces = vec![];
    let mut workspaces_bin_names = vec![];
    let mut workspaces_targets = vec![];

    if let Some(workspace) = cargo_toml.workspace {
        if let Some(members) = workspace.members {
            for member in members {
                let member_path = source_path.join(member.clone());
                let member_cargo_toml_path = member_path.join("Cargo.toml");

                if !member_cargo_toml_path.exists() {
                    bail!("Cargo.toml not found: {:?}", member_cargo_toml_path);
                }

                let member_cargo = read_cargo(member_cargo_toml_path.to_str().unwrap())?;

                let mut member_target_paths = vec![];

                if let Some(bins) = member_cargo.bin {
                    for bin in bins {
                        member_target_paths.push(format!("{}/{}", member, bin.path));
                        workspaces_bin_names.push(bin.name);
                    }
                }

                if member_target_paths.is_empty() {
                    member_target_paths.push(format!("{}/src/lib.rs", member));
                }

                for member_path in member_target_paths {
                    workspaces_targets.push(member_path);
                }

                workspaces.push(member);
            }
        }
    }

    // 2. CREATE ARTIFACTS

    // Get rust toolchain artifact

    let toolchain = context
        .fetch_artifact(toolchain_hash(context.get_target())?)
        .await?;
    let toolchain_target = toolchain_target(context.get_target())?;
    let toolchain_version = toolchain_version();

    // Set environment variables

    let toolchain_name = format!("{}-{}", toolchain_version, toolchain_target);

    let step_environments = vec![
        "HOME=$VORPAL_WORKSPACE/home".to_string(),
        format!(
            "PATH={}",
            format!(
                "{}/toolchains/{}-{}/bin",
                get_env_key(&toolchain),
                toolchain_version,
                toolchain_target
            )
        ),
        format!("RUSTUP_HOME={}", get_env_key(&toolchain)),
        format!("RUSTUP_TOOLCHAIN={}", toolchain_name),
    ];

    // Create vendor artifact

    let mut vendor_cargo_paths = vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()];

    for workspace in workspaces.iter() {
        vendor_cargo_paths.push(format!("{}/Cargo.toml", workspace));
    }

    let vendor_step_script = formatdoc! {"
        mkdir -pv $HOME

        pushd ./source/{name}-vendor

        target_paths=({target_paths})

        for target_path in ${{target_paths[@]}}; do
            mkdir -pv \"$(dirname \"${{target_path}}\")\"
            touch \"${{target_path}}\"
        done

        mkdir -pv \"$VORPAL_OUTPUT/vendor\"

        cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

        echo \"$cargo_vendor\" > \"$VORPAL_OUTPUT/config.toml\"",
        target_paths = workspaces_targets.join(" "),
    };

    let mut vendor_step_artifacts = vec![toolchain.clone()];

    vendor_step_artifacts.extend(artifacts.clone());

    let vendor_step = step::shell(
        context,
        vendor_step_artifacts,
        step_environments.clone(),
        vendor_step_script,
    )
    .await?;

    let vendor_name = format!("{}-vendor", name);

    let vendor_source =
        ConfigArtifactSourceBuilder::new(vendor_name.clone(), source_path_str.clone())
            .with_excludes(vec![])
            .with_includes(vendor_cargo_paths.clone())
            .build();

    let vendor = ConfigArtifactBuilder::new(vendor_name)
        .with_source(vendor_source)
        .with_step(vendor_step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await?;

    // TODO: implement artifact for 'check` to pre-bake the vendor cache

    let mut source_excludes = vec!["target".to_string()];

    for exclude in excludes {
        source_excludes.push(exclude.to_string());
    }

    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path_str)
        .with_excludes(source_excludes)
        .build();

    let mut step_artifacts = vec![toolchain.clone(), vendor.clone()];

    step_artifacts.extend(artifacts.clone());

    let step_script = formatdoc! {"
        mkdir -pv $HOME

        pushd ./source/{name}

        mkdir -pv .cargo

        ln -sv \"{vendor}/config.toml\" .cargo/config.toml

        cargo build --offline --release

        cargo test --offline --release

        mkdir -pv \"$VORPAL_OUTPUT/bin\"

        bin_names=({bin_names})

        for bin_name in ${{bin_names[@]}}; do
            cp -pv \"target/release/${{bin_name}}\" \"$VORPAL_OUTPUT/bin/\"
        done",
        bin_names = workspaces_bin_names.join(" "),
        vendor = get_env_key(&vendor),
    };

    let step = step::shell(context, step_artifacts, step_environments, step_script).await?;

    ConfigArtifactBuilder::new(name.to_string())
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
