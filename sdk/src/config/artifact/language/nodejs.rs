use std::{collections::BTreeMap, path::Path};

use anyhow::{bail, Result};
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::ArtifactId;

use crate::config::{artifact::{add_artifact, get_artifact_envkey, toolchain::nodejs}, ArtifactSource, ConfigContext};

fn get_nodejs_toolchain_version<'a>() -> &'a str {
    "22.12.0"
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

    let env_paths = vec![
        format!("{}/bin", get_artifact_envkey(&node))
    ];

    add_artifact(
        context,
        vec![],
        BTreeMap::from([
            ("PATH", env_paths.join(":")),
        ]),
        name,
        formatdoc! {"
            mkdir -pv $HOME

            pushd ./source/{name}

            npm install

            npm run build

            npm pack --pack-destination \"$VORPAL_WORKSPACE\"
        "},
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
