use anyhow::Result;
use vorpal_sdk::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::language::rust::{Rust, RustDevelopmentEnvironment},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;

    let systems: [ArtifactSystem; 4] = [Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    RustDevelopmentEnvironment::new("example-shell", systems.to_vec())
        .build(ctx)
        .await?;

    Rust::new("example", systems.to_vec())
        .with_bins(vec!["example"])
        .with_includes(vec!["src/main.rs", "Cargo.lock", "Cargo.toml"])
        .build(ctx)
        .await?;

    ctx.run().await
}
