use anyhow::Result;
use vorpal_schema::vorpal::config::v0::Config;
use vorpal_sdk::config::{
    artifact::language::rust::{rust_package, rust_shell},
    get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let context = &mut get_context().await?;

    let vorpal = rust_package(context, "vorpal").await?;
    let vorpal_shell = rust_shell(context, "vorpal-shell").await?;

    context
        .run(Config {
            artifacts: vec![vorpal, vorpal_shell],
        })
        .await
}
