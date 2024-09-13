use crate::log;
use crate::nickel;
use anyhow::{bail, Result};
use std::path::Path;
use vorpal_schema::{
    api::package::{PackageSystem, PackageSystem::Unknown},
    get_package_system, Config,
};

pub async fn check_config<'a>(
    file: &'a str,
    package: Option<&'a str>,
    system: &'a str,
) -> Result<Config> {
    let config_system: PackageSystem = get_package_system(system);

    if config_system == Unknown {
        bail!("unknown system: {}", system);
    }

    log::print_system(system);

    let config_path = Path::new(file);

    if !config_path.exists() {
        bail!("config not found: {}", config_path.display());
    }

    log::print_config(config_path);

    let config = nickel::load_config(config_path, config_system).await?;

    let packages = config.packages.clone();

    if let Some(package) = package {
        if !packages.contains_key(package) {
            bail!("package not found: {}", package);
        }
    }

    Ok(config)
}
