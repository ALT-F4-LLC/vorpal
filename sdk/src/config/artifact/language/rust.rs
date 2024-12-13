use crate::config::{
    artifact::{
        add_artifact, get_artifact_envkey,
        shell::shell_artifact,
        toolchain::{cargo, protoc, rust_analyzer, rust_src, rust_std, rustc},
    },
    ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use toml::from_str;
use vorpal_schema::vorpal::artifact::v0::{ArtifactEnvironment, ArtifactId, ArtifactSource};

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

const TOOLCHAIN_VERSION: &str = "1.80.1";

fn read_cargo_toml(path: &str) -> Result<RustArtifactCargoToml> {
    let contents = fs::read_to_string(path).expect("Failed to read Cargo.toml");
    Ok(from_str(&contents).expect("Failed to parse Cargo.toml"))
}

pub async fn rust_toolchain(
    context: &mut ConfigContext,
    name: &str,
    cargo: &ArtifactId,
    rust_analyzer: Option<ArtifactId>,
    rust_src: &ArtifactId,
    rust_std: &ArtifactId,
    rustc: &ArtifactId,
) -> Result<ArtifactId> {
    let mut artifacts = vec![
        cargo.clone(),
        rust_src.clone(),
        rust_std.clone(),
        rustc.clone(),
    ];

    if let Some(rust_analyzer) = rust_analyzer {
        artifacts.push(rust_analyzer);
    }

    let mut component_paths = vec![];

    for component in &artifacts {
        component_paths.push(get_artifact_envkey(component));
    }

    add_artifact(
        context,
        artifacts,
        vec![],
        format!("{}-rust-toolchain", name).as_str(),
        formatdoc! {"
            components=({component_paths})

            for component in ${{components[@]}}; do
                cp -prv \"${{component}}/.\" \"$VORPAL_OUTPUT\"
            done

            rm -rf \"$VORPAL_OUTPUT/manifest.in\"
            touch \"$VORPAL_OUTPUT/manifest.in\"

            for component in ${{components[@]}}; do
                cat \"${{component}}/manifest.in\" >> \"$VORPAL_OUTPUT\"/manifest.in
            done",
            component_paths = component_paths.join(" "),
        },
        vec![],
        vec![
            "aarch64-linux",
            "aarch64-macos",
            "x86_64-linux",
            "x86_64-macos",
        ],
    )
    .await
}

pub async fn rust_shell(context: &mut ConfigContext, name: &str) -> Result<ArtifactId> {
    let protoc = protoc::artifact(context).await?;

    // Get toolchain artifacts
    let cargo = cargo::artifact(context, TOOLCHAIN_VERSION).await?;
    let rust_analyzer = rust_analyzer::artifact(context, TOOLCHAIN_VERSION).await?;
    let rust_src = rust_src::artifact(context, TOOLCHAIN_VERSION).await?;
    let rust_std = rust_std::artifact(context, TOOLCHAIN_VERSION).await?;
    let rustc = rustc::artifact(context, TOOLCHAIN_VERSION).await?;

    // Get toolchain artifacts
    let toolchain = rust_toolchain(
        context,
        name,
        &cargo,
        Some(rust_analyzer),
        &rust_src,
        &rust_std,
        &rustc,
    )
    .await?;

    // Create shell artifact
    shell_artifact(context, vec![protoc, toolchain], vec![], name).await
}

pub async fn rust_package<'a>(context: &mut ConfigContext, name: &'a str) -> Result<ArtifactId> {
    let protoc = protoc::artifact(context).await?;

    // Get toolchain artifacts
    let cargo = cargo::artifact(context, TOOLCHAIN_VERSION).await?;
    let rust_src = rust_src::artifact(context, TOOLCHAIN_VERSION).await?;
    let rust_std = rust_std::artifact(context, TOOLCHAIN_VERSION).await?;
    let rustc = rustc::artifact(context, TOOLCHAIN_VERSION).await?;

    // Get toolchain
    let rust_toolchain =
        rust_toolchain(context, name, &cargo, None, &rust_src, &rust_std, &rustc).await?;

    // Get the source path
    let source = ".";
    let source_path = Path::new(source).to_path_buf();

    if !source_path.exists() {
        bail!("Artifact `source.{}.path` not found: {:?}", name, source);
    }

    // Load root cargo.toml
    let source_cargo_path = source_path.join("Cargo.toml");

    if !source_cargo_path.exists() {
        bail!("Cargo.toml not found: {:?}", source_cargo_path);
    }

    let source_cargo = read_cargo_toml(source_cargo_path.to_str().unwrap())?;

    // Get list of binary targets
    let mut workspaces = vec![];
    let mut workspaces_bin_names = vec![];
    let mut workspaces_targets = vec![];

    if let Some(workspace) = source_cargo.workspace {
        if let Some(members) = workspace.members {
            for member in members {
                let member_path = source_path.join(member.clone());
                let member_cargo_path = member_path.join("Cargo.toml");

                if !member_cargo_path.exists() {
                    bail!("Cargo.toml not found: {:?}", member_cargo_path);
                }

                let member_cargo = read_cargo_toml(member_cargo_path.to_str().unwrap())?;

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

    // Set default systems
    let systems = vec![
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos",
    ];

    // Create vendor artifact
    let mut vendor_tomls = vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()];

    for workspace in workspaces.iter() {
        vendor_tomls.push(format!("{}/Cargo.toml", workspace));
    }

    let vendor = add_artifact(
        context,
        vec![rust_toolchain.clone()],
        vec![
            ArtifactEnvironment {
                key: "HOME".to_string(),
                value: "$VORPAL_WORKSPACE/home".to_string(),
            },
            ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!(
                    "{rust_toolchain}/bin",
                    rust_toolchain = get_artifact_envkey(&rust_toolchain),
                ),
            },
        ],
        format!("{}-vendor", name).as_str(),
        formatdoc! {"
            mkdir -pv $HOME

            pushd ./source/{source}

            target_paths=({target_paths})

            for target_path in ${{target_paths[@]}}; do
                mkdir -pv \"$(dirname \"${{target_path}}\")\"
                touch \"${{target_path}}\"
            done

            mkdir -pv \"$VORPAL_OUTPUT/vendor\"

            cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

            echo \"$cargo_vendor\" > \"$VORPAL_OUTPUT/config.toml\"",
            source = name,
            target_paths = workspaces_targets.join(" "),
        },
        vec![ArtifactSource {
            excludes: vec![],
            hash: None,
            includes: vendor_tomls.clone(),
            name: name.to_string(),
            path: source.to_string(),
        }],
        systems.clone(),
    )
    .await?;

    // TODO: implement artifact for 'check` to pre-bake the vendor cache

    // Create artifact
    add_artifact(
        context,
        vec![rust_toolchain.clone(), protoc.clone(), vendor.clone()],
        vec![
            ArtifactEnvironment {
                key: "HOME".to_string(),
                value: "$VORPAL_WORKSPACE/home".to_string(),
            },
            ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!(
                    "{protoc}/bin:{rust_toolchain}/bin",
                    rust_toolchain = get_artifact_envkey(&rust_toolchain),
                    protoc = get_artifact_envkey(&protoc),
                ),
            },
        ],
        name,
        formatdoc! {"
            mkdir -pv $HOME

            pushd ./source/{name}

            mkdir -pv .cargo

            ln -sv \"{vendor_envkey}/config.toml\" .cargo/config.toml

            cargo build --offline --release
            cargo test --offline --release

            mkdir -pv \"$VORPAL_OUTPUT/bin\"

            bin_names=({bin_names})

            for bin_name in ${{bin_names[@]}}; do
                cp -pv \"target/release/${{bin_name}}\" \"$VORPAL_OUTPUT/bin/\"
            done",
            bin_names = workspaces_bin_names.join(" "),
            vendor_envkey = get_artifact_envkey(&vendor),
        },
        vec![ArtifactSource {
            excludes: vec![],
            hash: None,
            includes: vec![],
            name: name.to_string(),
            path: source.to_string(),
        }],
        systems,
    )
    .await
}
