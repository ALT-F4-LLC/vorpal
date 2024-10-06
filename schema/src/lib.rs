use crate::api::package::PackageSystem;
use crate::api::package::PackageSystem::{Aarch64Linux, Aarch64Macos, X8664Linux, X8664Macos};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub mod api {
    pub mod package {
        tonic::include_proto!("vorpal.package.v0");
    }
    pub mod store {
        tonic::include_proto!("vorpal.store.v0");
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PackageSourceKind {
    Unknown,
    Local,
    Git,
    Http,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackageSource {
    pub excludes: Vec<String>,
    pub hash: Option<String>,
    pub includes: Vec<String>,
    pub strip_prefix: bool,
    pub uri: String,
}

impl PackageSource {
    pub fn sort_vecs(&mut self) {
        self.excludes.sort();
        self.includes.sort();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub environment: BTreeMap<String, String>,
    pub name: String,
    pub packages: Vec<Package>,
    pub script: String,
    pub sandbox: Option<Box<Package>>,
    pub source: BTreeMap<String, PackageSource>,
    pub systems: Vec<String>,
}

impl Package {
    pub fn sort_vecs(&mut self) {
        self.packages.sort_by(|a, b| a.name.cmp(&b.name));
        self.systems.sort();
        for package in &mut self.packages {
            package.sort_vecs();
        }
        for source in self.source.values_mut() {
            source.sort_vecs();
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub packages: BTreeMap<String, Package>,
}

impl Config {
    pub fn sort_vecs(&mut self) {
        for package in self.packages.values_mut() {
            package.sort_vecs();
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

pub fn get_source_type(source_uri: &str) -> PackageSourceKind {
    match source_uri {
        uri if Path::new(&uri).exists() => PackageSourceKind::Local,
        uri if uri.starts_with("git") => PackageSourceKind::Git,
        uri if uri.starts_with("http") => PackageSourceKind::Http,
        _ => PackageSourceKind::Unknown,
    }
}
