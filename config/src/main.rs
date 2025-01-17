use anyhow::Result;
use vorpal_sdk::config::get_context;

mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    // Get the context
    let context = &mut get_context().await?;

    // Create artifacts
    let artifacts = vec![
        vorpal::artifact(context).await?,
        vorpal::shell(context).await?,
    ];

    // Run the context
    context.run(artifacts).await
}
