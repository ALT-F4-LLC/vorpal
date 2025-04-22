use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{
        cargo, clippy, get_env_key, language::rust, rust_analyzer, rust_src, rust_std, rustc,
        rustfmt, step, ArtifactBuilder,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

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

    let toolchain_target = rust::toolchain_target(context.get_system())?;
    let toolchain_version = rust::toolchain_version();

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

    let step = step::shell(context, artifacts, vec![], step_script).await?;

    ArtifactBuilder::new("rust-toolchain")
        .with_step(step)
        .with_system(Aarch64Darwin)
        .with_system(Aarch64Linux)
        .with_system(X8664Darwin)
        .with_system(X8664Linux)
        .build(context)
        .await
}
