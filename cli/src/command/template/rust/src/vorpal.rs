use anyhow::Result;
use vorpal_sdk::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{
        get_env_key, language::rust::Rust, protoc::Protoc, rust_toolchain,
        rust_toolchain::RustToolchain, ProjectEnvironment,
    },
    context::get_context,
};

const SYSTEMS: [ArtifactSystem; 4] = [Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    // Dependency artifacts

    let protoc = Protoc::new().build(context).await?;
    let rust_toolchain = RustToolchain::new().build(context).await?;

    // Source artifact

    Rust::new("example", SYSTEMS.to_vec())
        .with_bins(vec!["example"])
        .with_includes(vec!["src/main.rs", "Cargo.lock", "Cargo.toml"])
        .build(context)
        .await?;

    // Project environment

    let rust_toolchain_target = rust_toolchain::target(context.get_system())?;
    let rust_toolchain_version = rust_toolchain::version();
    let rust_toolchain_name = format!("{}-{}", rust_toolchain_version, rust_toolchain_target);
    let rust_toolchain_bin = format!(
        "{}/toolchains/{}/bin",
        get_env_key(&rust_toolchain),
        rust_toolchain_name
    );

    ProjectEnvironment::new("example-shell", SYSTEMS.to_vec())
        .with_artifacts(vec![protoc, rust_toolchain.clone()])
        .with_environments(vec![
            format!("PATH={}", rust_toolchain_bin),
            format!("RUSTUP_HOME={}", get_env_key(&rust_toolchain)),
            format!("RUSTUP_TOOLCHAIN={}", rust_toolchain_name),
        ])
        .build(context)
        .await?;

    context.run().await
}
