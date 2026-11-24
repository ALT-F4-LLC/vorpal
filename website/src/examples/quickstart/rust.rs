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

    // -> 1. Activate: `source "$(vorpal build --path 'hello-world-dev')/bin/activate"`
    // -> 2. Deactivate: `deactivate`

    RustDevelopmentEnvironment::new("hello-world-dev", SYSTEMS)
        .build(&mut ctx)
        .await?;

    // -> 1. Build: `vorpal build 'hello-world'`
    // -> 2. Run: `$(vorpal build --path 'hello-world')/bin/hello-world`

    Rust::new("hello-world", SYSTEMS)
        .with_bins(vec!["hello-world"])
        .with_includes(vec!["src/main.rs", "Cargo.lock", "Cargo.toml"])
        .build(&mut ctx)
        .await?;

    ctx.run().await
}
