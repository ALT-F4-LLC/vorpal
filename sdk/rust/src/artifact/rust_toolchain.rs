use crate::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{
        cargo, clippy, get_env_key, rust_analyzer, rust_src, rust_std, rustc, rustfmt, step,
        Artifact,
    },
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

pub fn target(system: ArtifactSystem) -> Result<String> {
    let target = match system {
        Aarch64Darwin => "aarch64-apple-darwin",
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        X8664Darwin => "x86_64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        _ => bail!("unsupported 'rust-toolchain' system: {:?}", system),
    };

    Ok(target.to_string())
}

pub fn version() -> String {
    "1.89.0".to_string()
}

pub async fn build(context: &mut ConfigContext) -> Result<String> {
    let cargo = cargo::build(context).await?;
    let clippy = clippy::build(context).await?;
    let rust_analyzer = rust_analyzer::build(context).await?;
    let rust_src = rust_src::build(context).await?;
    let rust_std = rust_std::build(context).await?;
    let rustc = rustc::build(context).await?;
    let rustfmt = rustfmt::build(context).await?;

    let artifacts = vec![
        cargo,
        clippy,
        rust_analyzer,
        rust_src,
        rust_std,
        rustc,
        rustfmt,
    ];

    let mut toolchain_component_paths = vec![];

    for component in &artifacts {
        toolchain_component_paths.push(get_env_key(component));
    }

    let toolchain_target = target(context.get_system())?;
    let toolchain_version = version();

    let step_script = formatdoc! {"
        toolchain_dir=\"$VORPAL_OUTPUT/toolchains/{toolchain_version}-{toolchain_target}\"

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
        component_paths = toolchain_component_paths.join(" "),
    };

    let steps = vec![step::shell(context, artifacts, vec![], step_script, vec![]).await?];
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];
    let name = "rust-toolchain";

    Artifact::new(name, steps, systems)
        .with_aliases(vec![format!("{name}:{toolchain_version}")])
        .build(context)
        .await
}
