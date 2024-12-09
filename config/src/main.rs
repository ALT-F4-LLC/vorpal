use anyhow::Result;
use vorpal_schema::vorpal::config::v0::Config;
use vorpal_sdk::config::{artifact::language::rust::rust_artifact, get_context};

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    let artifact = rust_artifact(context, "vorpal").await?;

    context
        .run(Config {
            artifacts: vec![artifact],
        })
        .await
}
