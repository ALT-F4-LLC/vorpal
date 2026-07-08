use anyhow::Result;
use vorpal_sdk::{
    artifact::language::rust::{Rust, RustDevelopmentEnvironment},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;

    let systems = [
        "aarch64-darwin",
        "aarch64-linux",
        "x86_64-darwin",
        "x86_64-linux",
    ];

    RustDevelopmentEnvironment::new("example-shell", systems)
        .build(ctx)
        .await?;

    Rust::new("example", systems)
        .with_bins(vec!["example"])
        .with_includes(vec!["src/main.rs", "Cargo.lock", "Cargo.toml"])
        .build(ctx)
        .await?;

    ctx.run().await
}
