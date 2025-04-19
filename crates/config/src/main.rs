use anyhow::{bail, Result};
use vorpal_sdk::context::get_context;

mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    let artifact = context.get_artifact_name();

    match artifact {
        "vorpal" => vorpal::build(context).await?,
        "vorpal-process" => vorpal::build_process(context).await?,
        "vorpal-release" => vorpal::build_release(context).await?,
        "vorpal-shell" => vorpal::build_shell(context).await?,
        "vorpal-test" => vorpal::build_test(context).await?,
        _ => bail!("unknown artifact: {}", artifact),
    };

    context.run().await
}
