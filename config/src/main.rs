use anyhow::Result;
use vorpal_sdk::config::{
    artifact::language::rust::{rust_package, rust_shell},
    get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Get the context
    let context = &mut get_context().await?;

    // Create artifacts
    let artifacts = vec![
        rust_package(context, "vorpal").await?,
        rust_shell(context, "vorpal").await?,
    ];

    // Run the context
    context.run(artifacts).await
}
