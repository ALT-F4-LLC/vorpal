use crate::artifact::v0::{
    ArtifactSystem,
    ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};
use anyhow::{bail, Result};
use std::env::consts::{ARCH, OS};

pub mod agent {
    pub mod v0 {
        tonic::include_proto!("vorpal.agent.v0");
    }
}

pub mod archive {
    pub mod v0 {
        tonic::include_proto!("vorpal.archive.v0");
    }
}

pub mod artifact {
    pub mod v0 {
        tonic::include_proto!("vorpal.artifact.v0");
    }
}

pub mod worker {
    pub mod v0 {
        tonic::include_proto!("vorpal.worker.v0");
    }
}

pub fn system_default_str() -> String {
    let os = match OS {
        "macos" => "darwin",
        _ => OS,
    };

    format!("{}-{}", ARCH, os)
}

pub fn system_default() -> Result<ArtifactSystem> {
    let platform = system_default_str();

    system_from_str(&platform)
}

pub fn system_from_str(system: &str) -> Result<ArtifactSystem> {
    let system = match system {
        "aarch64-darwin" => Aarch64Darwin,
        "aarch64-linux" => Aarch64Linux,
        "x86_64-darwin" => X8664Darwin,
        "x86_64-linux" => X8664Linux,
        _ => bail!("unsupported system: {}", system),
    };

    Ok(system)
}
