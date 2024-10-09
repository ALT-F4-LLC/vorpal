use crate::package::{bash, build_package, glibc};
use anyhow::Result;
use std::collections::HashMap;
use vorpal_schema::vorpal::config::v0::Config;

mod cross_platform;
mod package;

fn main() -> Result<()> {
    let package_bash = bash::package()?;

    let config = Config {
        packages: HashMap::from([("default".to_string(), package_bash)]),
    };

    let config_json = serde_json::to_string_pretty(&config).expect("failed to serialize json");

    println!("{}", config_json);

    Ok(())
}
