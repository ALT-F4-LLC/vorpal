use anyhow::Result;
use vorpal_sdk::{
    artifact::{language::rust::RustDevelopmentEnvironment, system},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let mut ctx = get_context().await?;

    // protoc is included by default; without_protoc() removes it
    RustDevelopmentEnvironment::new("my-project-dev", system::SYSTEMS)
        .with_environments(vec!["RUST_BACKTRACE=1".into()])
        .build(&mut ctx)
        .await?;

    ctx.run().await
}
