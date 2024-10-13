use anyhow::{bail, Result};
use vorpal_schema::vorpal::package::v0::PackageSystem;

pub fn get_cpu_count(target: PackageSystem) -> Result<String> {
    let system_cpus = match target {
        PackageSystem::Aarch64Linux => "nproc".to_string(),
        PackageSystem::Aarch64Macos => "sysctl -n hw.ncpu".to_string(),
        PackageSystem::X8664Linux => "nproc".to_string(),
        PackageSystem::X8664Macos => "sysctl -n hw.ncpu".to_string(),
        _ => bail!("unsupported system: {}", target.as_str_name()),
    };

    Ok(system_cpus)
}

pub fn get_sed_cmd(system: PackageSystem) -> Result<String> {
    let system_sed = match system {
        PackageSystem::Aarch64Linux => "sed -i".to_string(),
        PackageSystem::Aarch64Macos => "sed -i ''".to_string(),
        PackageSystem::X8664Linux => "sed -i".to_string(),
        PackageSystem::X8664Macos => "sed -i ''".to_string(),
        _ => bail!("unsupported system: {}", system.as_str_name()),
    };

    Ok(system_sed)
}
