use anyhow::Result;

mod command;

#[tokio::main]
async fn main() -> Result<()> {
    // Test change for cache invalidation
    command::run().await
}
