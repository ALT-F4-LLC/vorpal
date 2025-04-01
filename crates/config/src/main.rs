use anyhow::Result;
use vorpal_sdk::context::get_context;

mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Get the context

    let context = &mut get_context().await?;

    // 2. Create artifacts

    let mut artifacts = vec![];

    let vorpal = vorpal::build(context).await?;
    let vorpal_shell = vorpal::build_shell(context).await?;

    artifacts.push(vorpal);
    artifacts.push(vorpal_shell);

    // 3. Run the context

    context.run(artifacts).await
}
