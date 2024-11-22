use crate::{
    artifact::{build_artifact, cargo, new_source, protoc, rustc, step_env_artifact, zlib},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{ArtifactEnvironment, ArtifactId, ArtifactSystem};

pub struct ArtifactRust<'a> {
    pub cargo_hash: &'a str,
    pub name: &'a str,
    pub source: &'a str,
    pub source_excludes: Vec<&'a str>,
    pub systems: Vec<ArtifactSystem>,
}

pub fn build_rust_artifact(
    context: &mut ContextConfig,
    artifact: ArtifactRust,
) -> Result<ArtifactId> {
    let cargo = cargo::artifact(context)?;
    let rustc = rustc::artifact(context)?;
    let protoc = protoc::artifact(context)?;
    let zlib = zlib::artifact(context)?;

    let systems = artifact
        .systems
        .iter()
        .map(|s| (*s).into())
        .collect::<Vec<i32>>();

    let cargo_vendor_source = new_source(
        vec![],
        Some(artifact.cargo_hash.to_string()),
        vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "cli/Cargo.toml".to_string(),
            "config/Cargo.toml".to_string(),
            "notary/Cargo.toml".to_string(),
            "schema/Cargo.toml".to_string(),
            "store/Cargo.toml".to_string(),
            "worker/Cargo.toml".to_string(),
        ],
        artifact.name.to_string(),
        artifact.source.to_string(),
    )?;

    let cargo_vendor = build_artifact(
        context,
        vec![cargo.clone(), rustc.clone()],
        vec![ArtifactEnvironment {
            key: "PATH".to_string(),
            value: format!(
                "${cargo}/bin:${rustc}/bin",
                cargo = cargo.name.to_lowercase().replace("-", "_"),
                rustc = rustc.name.to_lowercase().replace("-", "_")
            ),
        }],
        format!("{}-cargo-vendor", artifact.name),
        formatdoc! {"
            dirs=(\"cli/src\" \"config/src\" \"notary/src\" \"schema/src\" \"store/src\" \"worker/src\")

            cd {source}

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
            source = artifact.name,
        },
        vec![cargo_vendor_source],
        systems.clone(),
    )?;

    let mut artifact_excludes = vec![
        ".git".to_string(),
        ".gitignore".to_string(),
        "target".to_string(),
    ];

    artifact_excludes.extend(artifact.source_excludes.iter().map(|e| e.to_string()));

    let artifact_source = new_source(
        artifact_excludes.clone(),
        None,
        vec![],
        artifact.name.to_string(),
        artifact.source.to_string(),
    )?;

    build_artifact(
        context,
        vec![
            cargo.clone(),
            cargo_vendor.clone(),
            protoc.clone(),
            rustc.clone(),
            zlib.clone(),
        ],
        vec![
            ArtifactEnvironment {
                key: "LD_LIBRARY_PATH".to_string(),
                value: format!("${}/usr/lib", zlib.name.to_lowercase().replace("-", "_")),
            },
            ArtifactEnvironment {
                key: "PATH".to_string(),
                value: format!(
                    "${cargo}/bin:${rustc}/bin:${protoc}/bin",
                    cargo = cargo.name.to_lowercase().replace("-", "_"),
                    protoc = protoc.name.to_lowercase().replace("-", "_"),
                    rustc = rustc.name.to_lowercase().replace("-", "_")
                ),
            },
        ],
        artifact.name.to_string(),
        formatdoc! {"
            cd {name}

            mkdir -p .cargo

            ln -sv \"${vendor_cache}/config.toml\" .cargo/config.toml

            cargo build --offline --release

            cargo test --offline --release

            mkdir -p \"$VORPAL_OUTPUT/bin\"

            cp -pr target/release/{name} $VORPAL_OUTPUT/bin/{name}",
            name = artifact.name,
            vendor_cache = step_env_artifact(&cargo_vendor),
        },
        vec![artifact_source],
        systems,
    )
}
