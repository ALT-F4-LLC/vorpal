use anyhow::Result;
use vorpal::command;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    command::run().await
}
