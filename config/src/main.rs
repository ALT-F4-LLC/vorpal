use anyhow::Result;
use vorpal_sdk::config::context::get_context;

mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Get the context
    let context = &mut get_context().await?;

    // 2. Create artifacts
    let artifacts = vec![
        vorpal::artifact(context).await?,
        vorpal::shell(context).await?,
    ];

    // 3. Run the context
    context.run(artifacts).await
}
