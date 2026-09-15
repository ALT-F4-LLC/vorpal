//! Vorpal CLI entry point.

use anyhow::Result;

mod command;
mod output;

#[tokio::main]
async fn main() -> Result<()> {
    Box::pin(command::run()).await
}
