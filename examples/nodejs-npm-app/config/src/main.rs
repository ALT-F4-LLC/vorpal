use anyhow::Result;
use vorpal_sdk::config::{
    artifact::language::nodejs::nodejs_package,
    get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Get the context
    let context = &mut get_context().await?;

    // Create artifacts
    let artifacts = vec![
        nodejs_package(context, "nodejs_example").await?,
    ];

    // Run the context
    context.run(artifacts).await
}
