use crate::artifact::linux_vorpal;
use anyhow::Result;
use vorpal_schema::config::v0::ConfigArtifactSystem::{Aarch64Linux, X8664Linux};
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

    let mut artifacts = vec![];

    if context_target == Aarch64Linux || context_target == X8664Linux {
        let linux_vorpal = linux_vorpal::artifact(context).await?;

        artifacts.push(linux_vorpal);
    }

    let devshell = vorpal::devshell(context).await?;
    // let package = vorpal::package(context).await?;

    artifacts.push(devshell);
    // artifacts.push(package);

    // 3. Run the context

    context.run(artifacts).await
}
