use crate::vorpal::artifact::v0::{
    ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};

pub mod vorpal {
    pub mod artifact {
        pub mod v0 {
            tonic::include_proto!("vorpal.artifact.v0");
        }
    }

    pub mod config {
        pub mod v0 {
            tonic::include_proto!("vorpal.config.v0");
        }
    }

    pub mod registry {
        pub mod v0 {
            tonic::include_proto!("vorpal.registry.v0");
        }
    }
}

pub trait ArtifactTarget {
    fn from_str(system: &str) -> Self;
}

impl ArtifactTarget for ArtifactSystem {
    fn from_str(system: &str) -> Self {
        match system {
            "aarch64-linux" => Aarch64Linux,
            "aarch64-macos" => Aarch64Macos,
            "x86_64-linux" => X8664Linux,
            "x86_64-macos" => X8664Macos,
            _ => ArtifactSystem::default(),
        }
    }
}

pub fn get_artifact_system<T: ArtifactTarget>(target: &str) -> T {
    T::from_str(target)
}
