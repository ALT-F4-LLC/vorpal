use crate::package::{bash, glibc};
use anyhow::Result;
use std::collections::HashMap;

mod cli;
mod cross_platform;
mod package;
mod service;

#[tokio::main]
async fn main() -> Result<()> {
    let packages = HashMap::from([("default".to_string(), bash::package()?)]);

    cli::execute(packages).await
}
