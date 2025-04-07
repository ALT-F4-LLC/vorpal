use anyhow::{bail, Result};
use std::env::consts::{ARCH, OS};
use vorpal_schema::artifact::v0::{
    ArtifactSystem,
    ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};

pub fn get_system_default_str() -> String {
    let os = match OS {
        "macos" => "darwin",
        _ => OS,
    };

    format!("{}-{}", ARCH, os)
}

pub fn get_system_default() -> Result<ArtifactSystem> {
    let platform = get_system_default_str();

    get_system(&platform)
}

pub fn get_system(system: &str) -> Result<ArtifactSystem> {
    let system = match system {
        "aarch64-darwin" => Aarch64Darwin,
        "aarch64-linux" => Aarch64Linux,
        "x86_64-darwin" => X8664Darwin,
        "x86_64-linux" => X8664Linux,
        _ => bail!("unsupported system: {}", system),
    };

    Ok(system)
}
