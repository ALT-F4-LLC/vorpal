use anyhow::Result;
use vorpal_schema::vorpal::artifact::v0::ArtifactSystem::{Aarch64Linux, X8664Linux};
use vorpal_sdk::context::get_context;

mod artifact;
mod source;
mod vorpal;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Get the context
    let context = &mut get_context().await?;

    let context_target = context.get_target();

    // 2. Create artifacts
    let mut artifacts = vec![artifact::protoc::artifact(context).await?];

    if context_target == Aarch64Linux || context_target == X8664Linux {
        let linux_debian = artifact::linux_debian::artifact(context).await?;
        let linux_vorpal = artifact::linux_vorpal::artifact(context, &linux_debian).await?;

        artifacts.push(linux_vorpal);
    }

    let devshell = vorpal::devshell(context).await?;
    let package = vorpal::package(context).await?;

    artifacts.push(devshell);
    artifacts.push(package);

    // 3. Run the context
    context.run(artifacts).await
}
