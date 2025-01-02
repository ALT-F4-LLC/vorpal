use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

use crate::config::{
    artifact::{add_artifact, get_artifact_envkey, toolchain::nodejs},
    ArtifactSource, ConfigContext,
};

fn get_nodejs_toolchain_version<'a>() -> &'a str {
    "22.12.0"
}

async fn npm_dependencies<'a>(
    context: &mut ConfigContext,
    name: &'a str,
    systems: Vec<&str>,
    node: &ArtifactId,
    source_path: PathBuf,
) -> Result<ArtifactId> {
    add_artifact(
        context,
        vec![],
        BTreeMap::from([("PATH", format!("{}/bin", get_artifact_envkey(node)))]),
        &format!("{}-node-modules", name),
        formatdoc! {"
            pushd ./source/{name}
            npm install
            cp -apv node_modules/. \"$VORPAL_OUTPUT/\"
        "},
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![],
                hash: None,
                includes: vec![
                    String::from(".npmrc"),
                    String::from("package.json"),
                    String::from("package-lock.json"),
                ],
                path: source_path.display().to_string(),
            },
        )]),
        systems,
    )
    .await
}

pub async fn nodejs_package<'a>(context: &mut ConfigContext, name: &'a str) -> Result<ArtifactId> {
    // 1. READ PACKAGE.JSON FILES

    // Get the source path
    let source_path = Path::new(".").to_path_buf();

    if !source_path.exists() {
        bail!(
            "Artifact `source.{}.path` not found: {:?}",
            name,
            source_path
        );
    }

    //let package_json_path = source_path.join("package.json");
    //let package_lock_path = source_path.join("package-lock.json");

    // Set default systems
    let systems = vec![
        "aarch64-linux",
        "aarch64-macos",
        "x86_64-linux",
        "x86_64-macos",
    ];

    let toolchain_version = get_nodejs_toolchain_version();

    let node = nodejs::artifact(context, toolchain_version).await?;

    let node_modules =
        npm_dependencies(context, name, systems.clone(), &node, source_path.clone()).await?;

    add_artifact(
        context,
        vec![node_modules.clone()],
        BTreeMap::from([("PATH", format!("{}/bin", get_artifact_envkey(&node)))]),
        name,
        formatdoc! {"
            pushd ./source/{name}
            ln -sf \"{node_modules_envkey}\" ./node_modules
            npm run build
            unlink ./node_modules
            npm pack --pack-destination \"$VORPAL_OUTPUT\"
            ln -sf \"{node_modules_envkey}\" \"$VORPAL_OUTPUT/node_modules\"
        ",
            node_modules_envkey = get_artifact_envkey(&node_modules),
        },
        BTreeMap::from([(
            name,
            ArtifactSource {
                excludes: vec![
                    ".env".to_string(),
                    ".envrc".to_string(),
                    ".github".to_string(),
                    ".gitignore".to_string(),
                    "Dockerfile".to_string(),
                    "dist".to_string(),
                    "node_modules".to_string(),
                    "target".to_string(),
                ],
                hash: None,
                includes: vec![],
                path: source_path.display().to_string(),
            },
        )]),
        systems,
    )
    .await
}
