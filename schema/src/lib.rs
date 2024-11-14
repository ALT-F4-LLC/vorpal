use crate::vorpal::artifact::v0::{
    ArtifactSourceKind,
    ArtifactSourceKind::{Git, Http, Local, UnknownKind},
    ArtifactSystem,
    ArtifactSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos},
};
use std::path::Path;

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

    pub mod store {
        pub mod v0 {
            tonic::include_proto!("vorpal.store.v0");
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

pub fn get_source_type(source_uri: &str) -> ArtifactSourceKind {
    match source_uri {
        uri if Path::new(&uri).exists() => Local,
        uri if uri.starts_with("git") => Git,
        uri if uri.starts_with("http") => Http,
        _ => UnknownKind,
    }
}
