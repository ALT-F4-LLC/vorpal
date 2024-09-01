use crate::api::package::PackageSystem;
use crate::api::package::PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod api {
    pub mod package {
        tonic::include_proto!("vorpal.package.v0");
    }
    pub mod store {
        tonic::include_proto!("vorpal.store.v0");
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub environment: HashMap<String, String>,
    pub name: String,
    pub packages: Vec<Package>,
    pub sandbox_image: String,
    pub script: String,
    pub source: Option<String>,
    pub source_excludes: Vec<String>,
    pub source_hash: Option<String>,
    pub source_includes: Vec<String>,
    pub systems: Vec<String>,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub packages: HashMap<String, Package>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_target_from_str() {
        let pairs = vec![
            ("aarch64-linux", Aarch64Linux),
            ("aarch64-macos", Aarch64Macos),
            ("x86_64-linux", X8664Linux),
            ("x86_64-macos", X8664Macos),
            ("unknown", PackageSystem::default()),
            ("armv6l-linux", PackageSystem::default()),
            ("armv7l-linux", PackageSystem::default()),
            ("i686-linux", PackageSystem::default()),
            ("mipsel-linux", PackageSystem::default()),
            ("armv5tel-linux", PackageSystem::default()),
            ("powerpc64le-linux", PackageSystem::default()),
            ("riscv64-linux", PackageSystem::default()),
            ("x86_64-freebsd", PackageSystem::default()),
        ];

        for (arch, expected) in pairs {
            assert_eq!(PackageSystem::from_str(arch), expected);
        }
    }
}
