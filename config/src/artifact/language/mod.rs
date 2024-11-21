use crate::{
    artifact::{build_artifact, cargo, get_sed_cmd, protoc, rustc, zlib},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactEnvironment, ArtifactId, ArtifactSource, ArtifactSystem,
};

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

    let name_envkey = artifact.name.to_lowercase().replace("-", "_");

    let artifact_cache = build_artifact(
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
        Some(artifact.cargo_hash.to_string()),
        format!("cache-{}", artifact.name),
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

            mkdir -p \"$output/vendor\"

            export CARGO_VENDOR=$(cargo vendor --versioned-dirs $output/vendor)
            echo \"$CARGO_VENDOR\" > \"$output/config.toml\"

            {sed} \"s|$output|${envkey}|g\" \"$output/config.toml\"",
            envkey = format!("cache_{}", name_envkey),
            sed = get_sed_cmd(context.get_target())?,
            source = artifact.name,
        },
        vec![ArtifactSource {
            excludes: vec![],
            includes: vec![
                "Cargo.lock".to_string(),
                "Cargo.toml".to_string(),
                "cli/Cargo.toml".to_string(),
                "config/Cargo.toml".to_string(),
                "notary/Cargo.toml".to_string(),
                "schema/Cargo.toml".to_string(),
                "store/Cargo.toml".to_string(),
                "worker/Cargo.toml".to_string(),
            ],
            name: artifact.name.to_string(),
            path: artifact.source.to_string(),
        }],
        systems.clone(),
    )?;

    let mut artifact_excludes = vec![
        ".git".to_string(),
        ".gitignore".to_string(),
        "target".to_string(),
    ];

    artifact_excludes.extend(artifact.source_excludes.iter().map(|e| e.to_string()));

    build_artifact(
        context,
        vec![
            cargo.clone(),
            rustc.clone(),
            protoc.clone(),
            zlib.clone(),
            artifact_cache,
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
        None,
        artifact.name.to_string(),
        formatdoc! {"
            cd {name}

            mkdir -p .cargo

            ln -sv \"$cache_{name_envkey}/config.toml\" .cargo/config.toml

            cargo build --offline --release
            cargo test --offline --release

            mkdir -p \"$output/bin\"

            cp -pr target/release/{name} $output/bin/{name}",
            name = artifact.name,
            name_envkey = name_envkey,
        },
        vec![ArtifactSource {
            excludes: artifact_excludes,
            includes: vec![],
            name: artifact.name.to_string(),
            path: artifact.source.to_string(),
        }],
        systems,
    )
}
