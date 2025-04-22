use anyhow::Result;

mod command;

#[tokio::main]
async fn main() -> Result<()> {
    command::run().await
}
