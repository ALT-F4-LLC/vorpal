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
pub struct LockfileSandbox {
    #[serde(rename = "aarch64-linux")]
    pub aarch64_linux: String,

    #[serde(rename = "aarch64-macos")]
    pub aarch64_macos: String,

    #[serde(rename = "x86_64-linux")]
    pub x8664_linux: String,

    #[serde(rename = "x86_64-macos")]
    pub x8664_macos: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lockfile {
    pub sandbox: LockfileSandbox,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub environment: HashMap<String, String>,
    pub name: String,
    pub packages: Vec<Package>,
    pub script: String,
    pub source: Option<String>,
    pub source_excludes: Vec<String>,
    pub source_hash: Option<String>,
    pub source_includes: Vec<String>,
    pub systems: Vec<String>,
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
