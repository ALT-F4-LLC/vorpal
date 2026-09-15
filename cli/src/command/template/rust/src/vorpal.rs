use anyhow::Result;
use vorpal_sdk::{
    artifact::{
        language::rust::{Rust, RustDevelopmentEnvironment},
        system::SYSTEMS,
    },
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut ctx = get_context().await?;

    // -> 1. Activate: `source "$(vorpal build --path 'example-dev')/bin/activate"`
    // -> 2. Deactivate: `deactivate`

    RustDevelopmentEnvironment::new("example-dev", SYSTEMS)
        .build(&mut ctx)
        .await?;

    // -> 1. Build: `vorpal build 'example'`
    // -> 2. Run: `$(vorpal build --path 'example')/bin/example`

    Rust::new("example", SYSTEMS)
        .with_bins(vec!["example"])
        .with_includes(vec!["src/main.rs", "Cargo.lock", "Cargo.toml"])
        .build(&mut ctx)
        .await?;

    ctx.run().await
}
