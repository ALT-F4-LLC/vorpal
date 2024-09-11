use crate::log;
use crate::nickel;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::Path;
use vorpal_schema::{
    api::package::{PackageSystem, PackageSystem::Unknown},
    get_package_system, Package,
};

pub async fn check_config<'a>(
    file: &'a str,
    package: Option<&'a str>,
    system: &'a str,
) -> Result<(HashMap<String, Package>, Vec<String>, String)> {
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

    let (config, config_hash) = nickel::load_config(config_path, config_system).await?;

    let packages = config.packages.clone();

    if let Some(package) = package {
        if !packages.contains_key(package) {
            bail!("package not found: {}", package);
        }
    }

    let (build_map, build_order) = nickel::load_config_build(&packages)?;

    log::print_packages(&build_map, &build_order);

    Ok((build_map, build_order, config_hash))
}
