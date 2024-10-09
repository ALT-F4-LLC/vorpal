use crate::vorpal::package::v0::PackageSystem;
use crate::vorpal::package::v0::PackageSystem::{
    Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos,
};
use std::path::Path;

pub mod vorpal {
    pub mod config {
        pub mod v0 {
            tonic::include_proto!("vorpal.config.v0");
        }
    }

    pub mod package {
        pub mod v0 {
            tonic::include_proto!("vorpal.package.v0");
        }
    }

    pub mod store {
        pub mod v0 {
            tonic::include_proto!("vorpal.store.v0");
        }
    }

    pub mod worker {
        pub mod v0 {
            tonic::include_proto!("vorpal.worker.v0");
        }
    }
}

pub trait PackageTarget {
    fn from_str(system: &str) -> Self;
}

impl PackageTarget for PackageSystem {
    fn from_str(system: &str) -> Self {
        match system {
            "aarch64-linux" => Aarch64Linux,
            "aarch64-macos" => Aarch64Macos,
            "x86_64-linux" => X8664Linux,
            "x86_64-macos" => X8664Macos,
            _ => PackageSystem::default(),
        }
    }
}

pub fn get_package_system<T: PackageTarget>(system: &str) -> T {
    T::from_str(system)
}

pub fn get_source_type(source_uri: &str) -> vorpal::package::v0::PackageSourceKind {
    match source_uri {
        uri if Path::new(&uri).exists() => vorpal::package::v0::PackageSourceKind::Local,
        uri if uri.starts_with("git") => vorpal::package::v0::PackageSourceKind::Git,
        uri if uri.starts_with("http") => vorpal::package::v0::PackageSourceKind::Http,
        _ => vorpal::package::v0::PackageSourceKind::UnknownKind,
    }
}
