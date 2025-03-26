use crate::artifact::{cargo, clippy, rust_analyzer, rust_src, rust_std, rustc, rustfmt};
use anyhow::{bail, Result};
use indoc::formatdoc;
use std::collections::BTreeMap;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactId, ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, UnknownSystem, X8664Linux, X8664Macos},
};
use vorpal_sdk::{
    artifact::{add_artifact, get_artifact_envkey},
    context::ConfigContext,
};

pub fn get_rust_toolchain_version() -> String {
    "1.83.0".to_string()
}

pub fn get_rust_toolchain_target(target: ArtifactSystem) -> Result<String> {
    let target = match target {
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        Aarch64Macos => "aarch64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        X8664Macos => "x86_64-apple-darwin",
        UnknownSystem => bail!("Unsupported rustc target: {:?}", target),
    };

    Ok(target.to_string())
}

pub async fn artifact(context: &mut ConfigContext) -> Result<ArtifactId> {
    let version = get_rust_toolchain_version();
    let target = get_rust_toolchain_target(context.get_target())?;

    let cargo = cargo::artifact(context, &version).await?;
    let clippy = clippy::artifact(context, &version).await?;
    let rust_analyzer = rust_analyzer::artifact(context, &version).await?;
    let rust_src = rust_src::artifact(context, &version).await?;
    let rust_std = rust_std::artifact(context, &version).await?;
    let rustc = rustc::artifact(context, &version).await?;
    let rustfmt = rustfmt::artifact(context, &version).await?;

    let artifacts = vec![
        cargo.clone(),
        clippy.clone(),
        rust_analyzer.clone(),
        rust_src.clone(),
        rust_std.clone(),
        rustc.clone(),
        rustfmt.clone(),
    ];

    let mut component_paths = vec![];

    for component in &artifacts {
        component_paths.push(get_artifact_envkey(component));
    }

    add_artifact(
        context,
        artifacts,
        BTreeMap::new(),
        "rust-toolchain",
        formatdoc! {"
            toolchain_dir=\"$VORPAL_OUTPUT/toolchains/{version}-{target}\"

            mkdir -pv \"$toolchain_dir\"

            components=({component_paths})

            for component in \"${{components[@]}}\"; do
                find \"$component\" | while read -r file; do
                    relative_path=$(echo \"$file\" | sed -e \"s|$component||\")

                    echo \"Copying $file to $toolchain_dir$relative_path\"

                    if [[ \"$relative_path\" == \"/manifest.in\" ]]; then
                        continue
                    fi

                    if [ -d \"$file\" ]; then
                        mkdir -pv \"$toolchain_dir$relative_path\"
                    else
                        cp -pv \"$file\" \"$toolchain_dir$relative_path\"
                    fi
                done
            done

            cat > \"$VORPAL_OUTPUT/settings.toml\" << \"EOF\"
            auto_self_update = \"disable\"
            profile = \"minimal\"
            version = \"12\"

            [overrides]
            EOF",
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
