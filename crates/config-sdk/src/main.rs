use crate::artifact::{linux_vorpal, protoc, rust_toolchain};
use anyhow::Result;
use vorpal_schema::config::v0::ConfigArtifactSystem::{Aarch64Linux, X8664Linux};
use vorpal_sdk::context::get_context;

mod artifact;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Get the context

    let context = &mut get_context().await?;
    let context_target = context.get_target();

    // 2. Create artifacts

    let mut artifacts = vec![];

    if context_target == Aarch64Linux || context_target == X8664Linux {
        let linux_vorpal = linux_vorpal::build(context).await?;

        artifacts.push(linux_vorpal);
    }

    let rust_toolchain = rust_toolchain::build(context).await?;
    let protoc = protoc::build(context).await?;

    artifacts.push(rust_toolchain);
    artifacts.push(protoc);

    // 3. Run the context

    context.run(artifacts).await
}
