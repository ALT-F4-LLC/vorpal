use anyhow::{bail, Result};
use std::env::consts::{ARCH, OS};
use vorpal_schema::{get_package_system, vorpal::package::v0::PackageSystem};

pub fn get_cpu_count() -> Result<String> {
    let system = format!("{}-{}", ARCH, OS);

    let system_target = get_package_system::<PackageSystem>(system.as_str());

    let system_cpus = match system_target {
        PackageSystem::Aarch64Linux => "nproc".to_string(),
        PackageSystem::Aarch64Macos => "sysctl -n hw.ncpu".to_string(),
        PackageSystem::X8664Linux => "nproc".to_string(),
        PackageSystem::X8664Macos => "sysctl -n hw.ncpu".to_string(),
        _ => bail!("unsupported system: {}", system),
    };

    Ok(system_cpus)
}

pub fn get_sed_cmd() -> Result<String> {
    let system = format!("{}-{}", ARCH, OS);

    let system_target = get_package_system::<PackageSystem>(system.as_str());

    let system_sed = match system_target {
        PackageSystem::Aarch64Linux => "sed -i ''".to_string(),
        PackageSystem::Aarch64Macos => "sed -i ''".to_string(),
        PackageSystem::X8664Linux => "sed -i".to_string(),
        PackageSystem::X8664Macos => "sed -i".to_string(),
        _ => bail!("unsupported system: {}", system),
    };

    Ok(system_sed)
}
