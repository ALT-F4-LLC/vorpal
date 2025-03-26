use crate::{
    artifact::{add_artifact, get_artifact_envkey, shell::shell_artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use toml::from_str;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
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

fn get_rust_toolchain_version() -> String {
    "1.83.0".to_string()
}

fn get_rust_toolchain_target(target: ArtifactSystem) -> Result<String> {
    let target = match target {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported rustc target: {:?}", target),
    };

    Ok(target.to_string())
}

pub async fn rust_shell(context: &mut ConfigContext, name: &str) -> Result<ArtifactId> {
    let rust_toolchain = context
        .fetch_artifact(
            "rust-toolchain",
            "12aba225f85e7310d03c83f2c137270b5127b42eb2db89d481a048f440a7aff5",
        )
        .await?;

    let rust_toolchain_target = get_rust_toolchain_target(context.get_target())?;
    let rust_toolchain_version = get_rust_toolchain_version();

    let protoc = context
        .fetch_artifact(
            "protoc",
            "1b919861ad528e32772b65a9aaefb41014adcdcc9b296e1e3510f4cecb07ef9e",
        )
        .await?;

    let artifacts = vec![protoc.clone(), rust_toolchain.clone()];

    let envs = vec![
        format!(
            "PATH={}/bin:{}/toolchains/{}-{}/bin:$PATH",
            get_artifact_envkey(&protoc),
            get_artifact_envkey(&rust_toolchain),
            rust_toolchain_version,
            rust_toolchain_target
        ),
        format!("RUSTUP_HOME={}", get_artifact_envkey(&rust_toolchain)),
        format!(
            "RUSTUP_TOOLCHAIN={}-{}",
            rust_toolchain_version, rust_toolchain_target
        ),
    ];

    // Create shell artifact
    shell_artifact(context, artifacts, envs, name).await
}

pub async fn rust_package<'a>(
    context: &mut ConfigContext,
    name: &'a str,
    excludes: Vec<&'a str>,
) -> Result<ArtifactId> {
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

    // Load root cargo.toml
    let cargo_toml_path = source_path.join("Cargo.toml");

    if !cargo_toml_path.exists() {
        bail!("Cargo.toml not found: {:?}", cargo_toml_path);
    }

    let cargo_toml = read_cargo_toml(cargo_toml_path.to_str().unwrap())?;

    // TODO: implement for non-workspace based projects

    // Get list of binary targets
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

    // Set default systems
    let systems = vec![
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos",
    ];

    // Get protoc artifact
    let protoc = context
        .fetch_artifact(
            "protoc",
            "1b919861ad528e32772b65a9aaefb41014adcdcc9b296e1e3510f4cecb07ef9e",
        )
        .await?;

    // Get rust toolchain artifact

    let rust_toolchain = context
        .fetch_artifact(
            "rust-toolchain",
            "12aba225f85e7310d03c83f2c137270b5127b42eb2db89d481a048f440a7aff5",
        )
        .await?;

    let rust_toolchain_target = get_rust_toolchain_target(context.get_target())?;
    let rust_toolchain_version = get_rust_toolchain_version();

    // Set environment variables

    let mut env_paths: Vec<String> = vec![format!(
        "{}/toolchains/{}-{}/bin",
        get_artifact_envkey(&rust_toolchain),
        rust_toolchain_version,
        rust_toolchain_target
    )];

    let env_toolchain = format!("{}-{}", rust_toolchain_version, rust_toolchain_target);

    // Create vendor artifact

    let mut vendor_cargo_tomls = vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()];

    for workspace in workspaces.iter() {
        vendor_cargo_tomls.push(format!("{}/Cargo.toml", workspace));
    }

    let vendor_source = context
        .add_artifact_source(
            format!("{}-vendor", name).as_str(),
            ArtifactSource {
                excludes: vec![],
                hash: None,
                includes: vendor_cargo_tomls.clone(),
                path: source_path.display().to_string(),
            },
        )
        .await?;

    let vendor = add_artifact(
        context,
        vec![/* toolchain.clone() */],
        BTreeMap::from([
            ("HOME", "$VORPAL_WORKSPACE/home".to_string()),
            ("PATH", env_paths.join(":")),
            ("RUSTUP_HOME", get_artifact_envkey(&rust_toolchain)),
            ("RUSTUP_TOOLCHAIN", env_toolchain),
        ]),
        format!("{}-vendor", name).as_str(),
        formatdoc! {"
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
        },
        vec![vendor_source],
        systems.clone(),
    )
    .await?;

    // TODO: implement artifact for 'check` to pre-bake the vendor cache

    let artifacts = vec![protoc.clone(), rust_toolchain.clone(), vendor.clone()];

    // Create artifact

    env_paths.push(format!("{}/bin", get_artifact_envkey(&protoc)));

    let mut excludes_defaults = vec!["target".to_string()];

    for exclude in excludes {
        excludes_defaults.push(exclude.to_string());
    }

    let artifact_source = context
        .add_artifact_source(
            name,
            ArtifactSource {
                excludes: excludes_defaults,
                hash: None,
                includes: vec![],
                path: source_path.display().to_string(),
            },
        )
        .await?;

    add_artifact(
        context,
        artifacts,
        BTreeMap::from([
            ("HOME", "$VORPAL_WORKSPACE/home".to_string()),
            ("PATH", env_paths.join(":")),
            ("RUSTUP_HOME", get_artifact_envkey(&rust_toolchain)),
            (
                "RUSTUP_TOOLCHAIN",
                format!("{}-{}", get_rust_toolchain_version(), rust_toolchain_target),
            ),
        ]),
        name,
        formatdoc! {"
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
            vendor = get_artifact_envkey(&vendor),
        },
        vec![artifact_source],
        systems,
    )
    .await
}
