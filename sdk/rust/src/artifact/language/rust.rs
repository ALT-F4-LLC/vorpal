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

fn read_cargo_toml(path: &str) -> Result<RustArtifactCargoToml> {
    let contents = fs::read_to_string(path).expect("Failed to read Cargo.toml");

    Ok(from_str(&contents).expect("Failed to parse Cargo.toml"))
}

pub fn get_toolchain_version() -> String {
    "1.83.0".to_string()
}

pub fn get_toolchain_target(target: ConfigArtifactSystem) -> Result<String> {
    let target = match target {
        Aarch64Darwin => "aarch64-apple-darwin",
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        X8664Darwin => "x86_64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        _ => bail!("unsupported rust-toolchain target: {:?}", target),
    };

    Ok(target.to_string())
}

pub async fn devshell(context: &mut ConfigContext, name: &str) -> Result<String> {
    let rust_toolchain = context
        .fetch_artifact("12aba225f85e7310d03c83f2c137270b5127b42eb2db89d481a048f440a7aff5")
        .await?;

    let rust_toolchain_target = get_toolchain_target(context.get_target())?;

    let rust_toolchain_version = get_toolchain_version();

    let protoc = context
        .fetch_artifact("1b919861ad528e32772b65a9aaefb41014adcdcc9b296e1e3510f4cecb07ef9e")
        .await?;

    let artifacts = vec![protoc.clone(), rust_toolchain.clone()];

    let environments = vec![
        format!(
            "PATH={}/bin:{}/toolchains/{}-{}/bin:$PATH",
            get_env_key(&protoc),
            get_env_key(&rust_toolchain),
            rust_toolchain_version,
            rust_toolchain_target
        ),
        format!("RUSTUP_HOME={}", get_env_key(&rust_toolchain)),
        format!(
            "RUSTUP_TOOLCHAIN={}-{}",
            rust_toolchain_version, rust_toolchain_target
        ),
    ];

    // Create shell artifact

    shell::build(context, artifacts, environments, name).await
}

pub async fn package<'a>(
    context: &mut ConfigContext,
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

    let cargo_toml = read_cargo_toml(cargo_toml_path.to_str().unwrap())?;

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

                let member_cargo = read_cargo_toml(member_cargo_toml_path.to_str().unwrap())?;

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

    // Get protoc artifact

    let protoc = context
        .fetch_artifact("1b919861ad528e32772b65a9aaefb41014adcdcc9b296e1e3510f4cecb07ef9e")
        .await?;

    // Get rust toolchain artifact

    let rust_toolchain = context
        .fetch_artifact("12aba225f85e7310d03c83f2c137270b5127b42eb2db89d481a048f440a7aff5")
        .await?;

    let rust_toolchain_target = get_toolchain_target(context.get_target())?;

    let rust_toolchain_version = get_toolchain_version();

    // Set environment variables

    let mut environments_paths: Vec<String> = vec![format!(
        "{}/toolchains/{}-{}/bin",
        get_env_key(&rust_toolchain),
        rust_toolchain_version,
        rust_toolchain_target
    )];

    let rustup_toolchain = format!("{}-{}", rust_toolchain_version, rust_toolchain_target);

    let shared_step_environments = vec![
        "HOME=$VORPAL_WORKSPACE/home".to_string(),
        format!("PATH={}", environments_paths.join(":")),
        format!("RUSTUP_HOME={}", get_env_key(&rust_toolchain)),
        format!("RUSTUP_TOOLCHAIN={}", rustup_toolchain),
    ];

    // Create vendor artifact

    let mut vendor_toml_paths = vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()];

    for workspace in workspaces.iter() {
        vendor_toml_paths.push(format!("{}/Cargo.toml", workspace));
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

    let vendor_step_artifacts = vec![protoc.clone(), rust_toolchain.clone()];

    let vendor_step = step::shell(
        context,
        vendor_step_artifacts,
        shared_step_environments.clone(),
        vendor_step_script,
    )
    .await?;

    let vendor_name = format!("{}-vendor", name);

    let vendor_source =
        ConfigArtifactSourceBuilder::new(vendor_name.clone(), source_path_str.clone())
            .with_excludes(vec![])
            .with_includes(vendor_toml_paths.clone())
            .build();

    let vendor = ConfigArtifactBuilder::new(vendor_name)
        .with_source(vendor_source)
        .with_step(vendor_step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)?;

    // TODO: implement artifact for 'check` to pre-bake the vendor cache

    let step_artifacts = vec![protoc.clone(), rust_toolchain.clone(), vendor.clone()];

    environments_paths.push(format!("{}/bin", get_env_key(&protoc)));

    let mut source_excludes = vec!["target".to_string()];

    for exclude in excludes {
        source_excludes.push(exclude.to_string());
    }

    let source = ConfigArtifactSourceBuilder::new(name.to_string(), source_path_str)
        .with_excludes(source_excludes)
        .build();

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

    let step = step::shell(
        context,
        step_artifacts,
        shared_step_environments,
        step_script,
    )
    .await?;

    ConfigArtifactBuilder::new(name.to_string())
        .with_source(source)
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
}
