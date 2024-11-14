use anyhow::{bail, Result};
use vorpal_schema::vorpal::artifact::v0::{
    ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

// pub fn get_cpu_count(target: ArtifactSystem) -> Result<String> {
//     let system_cpus = match target {
//         Aarch64Linux | X8664Linux => "nproc".to_string(),
//         Aarch64Macos | X8664Macos => "sysctl -n hw.ncpu".to_string(),
//         _ => bail!("unsupported system: {}", target.as_str_name()),
//     };
//
//     Ok(system_cpus)
// }

pub fn get_sed_cmd(system: ArtifactSystem) -> Result<String> {
    let system_sed = match system {
        Aarch64Linux | X8664Linux => "sed -i".to_string(),
        Aarch64Macos | X8664Macos => "sed -i ''".to_string(),
        _ => bail!("unsupported system: {}", system.as_str_name()),
    };

    Ok(system_sed)
}
