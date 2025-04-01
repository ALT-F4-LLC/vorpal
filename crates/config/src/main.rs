use anyhow::Result;
use vorpal_sdk::context::get_context;

mod artifact;
mod source;
mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Get the context

    let context = &mut get_context().await?;

    // 2. Create artifacts

    let artifacts = vec![
        vorpal::devshell(context).await?,
        vorpal::package(context).await?,
    ];

    // 3. Run the context

    context.run(artifacts).await
}
