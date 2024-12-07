use crate::config::{
    artifact::{
        add_artifact, add_artifact_source, get_artifact_envkey,
        toolchain::{cargo, protoc, rustc},
    },
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{ArtifactEnvironment, ArtifactId, ArtifactSystem};

pub fn artifact<'a>(
    context: &mut ContextConfig,
    excludes: Vec<&'a str>,
    name: &'a str,
    systems: Vec<ArtifactSystem>,
) -> Result<ArtifactId> {
    let cargo = cargo::artifact(context)?;
    let rustc = rustc::artifact(context)?;
    let protoc = protoc::artifact(context)?;

    let path = ".";

    let targets = systems.iter().map(|s| (*s).into()).collect::<Vec<i32>>();

    let vendors_source = add_artifact_source(
        vec![],
        None,
        vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "cli/Cargo.toml".to_string(),
            "config/Cargo.toml".to_string(),
            "notary/Cargo.toml".to_string(),
            "schema/Cargo.toml".to_string(),
            "sdk/Cargo.toml".to_string(),
            "store/Cargo.toml".to_string(),
            "worker/Cargo.toml".to_string(),
        ],
        name.to_string(),
        path.to_string(),
    )?;

    let vendors = add_artifact(
        context,
        vec![cargo.clone(), rustc.clone()],
        vec![
            ArtifactEnvironment {
                key: "HOME".to_string(),
                value: "$VORPAL_WORKSPACE/home".to_string(),
            },
            ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!(
                    "{cargo}/bin:{rustc}/bin",
                    cargo = get_artifact_envkey(&cargo),
                    rustc = get_artifact_envkey(&rustc)
                ),
            },
        ],
        format!("{}-cargo-vendor", name).as_str(),
        formatdoc! {"
            mkdir -pv $HOME

            dirs=(\"cli/src\" \"config/src\" \"notary/src\" \"schema/src\" \"sdk/src\" \"store/src\" \"worker/src\")

            pushd ./source/{source}

            for dir in \"${{dirs[@]}}\"; do
                mkdir -p \"$dir\"
            done

            for dir in \"${{dirs[@]}}\"; do
                if [[ \"$dir\" == \"cli/src\" || \"$dir\" == \"config/src\" ]]; then
                    touch \"$dir/main.rs\"
                else
                    touch \"$dir/lib.rs\"
                fi
            done

            mkdir -p \"$VORPAL_OUTPUT/vendor\"

            export CARGO_VENDOR=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)

            echo \"$CARGO_VENDOR\" > \"$VORPAL_OUTPUT/config.toml\"",
            source = name,
        },
        vec![vendors_source],
        targets.clone(),
    )?;

    // TODO: implement artifact for 'check` to pre-bake the vendor cache

    let mut artifact_excludes = vec![
        ".git".to_string(),
        ".gitignore".to_string(),
        "target".to_string(),
    ];

    artifact_excludes.extend(excludes.iter().map(|e| e.to_string()));

    let artifact_source = add_artifact_source(
        artifact_excludes.clone(),
        None,
        vec![],
        name.to_string(),
        path.to_string(),
    )?;

    add_artifact(
        context,
        vec![
            cargo.clone(),
            protoc.clone(),
            rustc.clone(),
            vendors.clone(),
        ],
        vec![
            ArtifactEnvironment {
                key: "HOME".to_string(),
                value: "$VORPAL_WORKSPACE/home".to_string(),
            },
            ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!(
                    "{cargo}/bin:{rustc}/bin:{protoc}/bin",
                    cargo = get_artifact_envkey(&cargo),
                    rustc = get_artifact_envkey(&rustc),
                    protoc = get_artifact_envkey(&protoc),
                ),
            },
        ],
        name,
        formatdoc! {"
            mkdir -pv $HOME

            pushd ./source/{name}

            mkdir -p .cargo

            ln -sv \"{vendor_cache}/config.toml\" .cargo/config.toml

            cargo build --offline --release

            cargo test --offline --release

            mkdir -p \"$VORPAL_OUTPUT/bin\"

            cp -pr ./target/release/. $VORPAL_OUTPUT/.",
            vendor_cache = get_artifact_envkey(&vendors),
        },
        vec![artifact_source],
        targets,
    )
}
