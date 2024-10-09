use crate::package::{bash, build_package, glibc};
use anyhow::Result;
use std::collections::BTreeMap;
use vorpal_schema::Config;

mod cross_platform;
mod package;

fn main() -> Result<()> {
    let package_bash = bash::package()?;

    let config = Config {
        packages: BTreeMap::from([("default".to_string(), package_bash)]),
    };

    let config_json = serde_json::to_string_pretty(&config).expect("failed to serialize json");

    println!("{}", config_json);

    Ok(())
}
