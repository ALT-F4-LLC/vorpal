use anyhow::{anyhow, Result};
use vorpal_sdk::context::get_context;

mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    match context.get_artifact_name() {
        "vorpal" => vorpal::package(context).await?,
        "vorpal-release" => vorpal::release(context).await?,
        "vorpal-shell" => vorpal::shell(context).await?,
        _ => return Err(anyhow!("unknown artifact: {}", context.get_artifact_name())),
    }

    context.run().await
}
