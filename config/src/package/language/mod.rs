use crate::{
    package::{build_package, cargo, get_sed_cmd, protoc, rustc, zlib},
    ContextConfig,
};
use anyhow::Result;
use indoc::formatdoc;
use vorpal_schema::vorpal::package::v0::{
    Package, PackageEnvironment, PackageOutput, PackageSource, PackageSystem,
};

pub struct PackageRust<'a> {
    pub cargo_hash: &'a str,
    pub name: &'a str,
    pub source: &'a str,
    pub source_excludes: Vec<&'a str>,
    pub systems: Vec<PackageSystem>,
}

pub fn build_rust_package(
    context: &mut ContextConfig,
    package: PackageRust,
) -> Result<PackageOutput> {
    let cargo = cargo::package(context)?;
    let rustc = rustc::package(context)?;
    let protoc = protoc::package(context)?;
    let zlib = zlib::package(context)?;

    let systems = package
        .systems
        .iter()
        .map(|s| (*s).into())
        .collect::<Vec<i32>>();

    let name_envkey = package.name.to_lowercase().replace("-", "_");

    let package_cache = build_package(
        context,
        Package {
            environment: vec![PackageEnvironment {
                key: "PATH".to_string(),
                value: format!(
                    "${cargo}/bin:${rustc}/bin",
                    cargo = cargo.name.to_lowercase().replace("-", "_"),
                    rustc = rustc.name.to_lowercase().replace("-", "_")
                ),
            }],
            name: format!("cache-{}", package.name),
            packages: vec![cargo.clone(), rustc.clone()],
            sandbox: None,
            script: formatdoc! {"
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
                envkey = format!("{}_cache", name_envkey),
                sed = get_sed_cmd(context.get_target())?,
                source = package.name,
            },
            source: vec![PackageSource {
                excludes: vec![],
                hash: Some(package.cargo_hash.to_string()),
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
                name: package.name.to_string(),
                strip_prefix: false,
                uri: package.source.to_string(),
            }],
            systems: systems.clone(),
        },
    )?;

    let mut package_excludes = vec![
        ".git".to_string(),
        ".gitignore".to_string(),
        "target".to_string(),
    ];

    package_excludes.extend(package.source_excludes.iter().map(|e| e.to_string()));

    let package = build_package(
        context,
        Package {
            environment: vec![
                PackageEnvironment {
                    key: "LD_LIBRARY_PATH".to_string(),
                    value: format!("${}/usr/lib", zlib.name.to_lowercase().replace("-", "_")),
                },
                PackageEnvironment {
                    key: "PATH".to_string(),
                    value: format!(
                        "${cargo}/bin:${rustc}/bin:${protoc}/bin",
                        cargo = cargo.name.to_lowercase().replace("-", "_"),
                        protoc = protoc.name.to_lowercase().replace("-", "_"),
                        rustc = rustc.name.to_lowercase().replace("-", "_")
                    ),
                },
            ],
            name: package.name.to_string(),
            packages: vec![cargo, rustc, protoc, zlib, package_cache],
            sandbox: None,
            script: formatdoc! {"
                cd {name}

                mkdir -p .cargo

                ln -sv \"$cache_{name_envkey}/config.toml\" .cargo/config.toml

                cargo build --offline --release
                cargo test --offline --release

                mkdir -p \"$output/bin\"

                cp -pr target/release/{name} $output/bin/{name}",
                name = package.name,
                name_envkey = name_envkey,
            },
            source: vec![PackageSource {
                excludes: package_excludes,
                hash: None,
                includes: vec![],
                name: package.name.to_string(),
                strip_prefix: false,
                uri: package.source.to_string(),
            }],
            systems,
        },
    )?;

    Ok(package)
}
