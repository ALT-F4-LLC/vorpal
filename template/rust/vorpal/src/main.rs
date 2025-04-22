use anyhow::Result;
use vorpal_sdk::{
    artifact::language::rust::{RustBuilder, RustShellBuilder},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    RustBuilder::new("example")
        .with_bins(vec!["example"])
        .with_packages(vec!["example"])
        .build(context)
        .await?;

    RustShellBuilder::new("example-shell")
        .build(context)
        .await?;

    context.run().await
}
