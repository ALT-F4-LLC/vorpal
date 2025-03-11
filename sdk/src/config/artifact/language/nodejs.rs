use std::{
    collections::BTreeMap,
    fs::read_to_string,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
use indoc::formatdoc;
use serde_json::Value;
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
        vec![node.clone()],
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

    let package_json_path = source_path.join("package.json");
    let package_json_str = read_to_string(package_json_path)?;
    let package_json: Value = serde_json::from_str(&package_json_str)?;
    let package_json_name = package_json["name"].clone();
    let package_json_version = package_json["version"].clone();

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
        vec![node_modules.clone(), node.clone()],
        BTreeMap::from([("PATH", format!("{}/bin", get_artifact_envkey(&node)))]),
        name,
        formatdoc! {"
mkdir -p \"$VORPAL_WORKSPACE/out\" \"$VORPAL_WORKSPACE/unpacked\"
pushd ./source/{name}

ln -sf \"{node_modules_envkey}\" ./node_modules
npm run build
unlink ./node_modules

npm pack --pack-destination \"$VORPAL_WORKSPACE/out\"

tar \
        -xf \
        \"$VORPAL_WORKSPACE/out/{package_json_name}-{package_json_version}.tgz\" \
    -C \"$VORPAL_WORKSPACE/unpacked\"

cp -apv \"$VORPAL_WORKSPACE/unpacked/package/.\" \"$VORPAL_OUTPUT/\"

ln -sf \"{node_modules_envkey}\" \"$VORPAL_OUTPUT/node_modules\"

echo << EOJ > \"$VORPAL_OUTPUT/wrapper.sh\"
#!/usr/bin/env sh
exec \"{node_envkey}/bin/node\" \"$VORPAL_OUTPUT\"
EOJ
chmod +x \"$VORPAL_OUTPUT/wrapper.sh\"
        ",
            node_modules_envkey = get_artifact_envkey(&node_modules),
            node_envkey = get_artifact_envkey(&node),
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
