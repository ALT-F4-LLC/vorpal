use crate::api::{ConfigPackageBuildSystem, PackageBuildSystem};

pub mod agent;
pub mod config;
pub mod package;
pub mod store;
pub mod worker;

pub trait BuildSystem {
    fn from_str(system: &str) -> Self;
}

impl BuildSystem for ConfigPackageBuildSystem {
    fn from_str(system: &str) -> Self {
        match system {
            "aarch64-linux" => ConfigPackageBuildSystem::Aarch64Linux,
            "aarch64-macos" => ConfigPackageBuildSystem::Aarch64Macos,
            "x86_64-linux" => ConfigPackageBuildSystem::X8664Linux,
            "x86_64-macos" => ConfigPackageBuildSystem::X8664Macos,
            _ => ConfigPackageBuildSystem::default(),
        }
    }
}

impl BuildSystem for PackageBuildSystem {
    fn from_str(system: &str) -> Self {
        match system {
            "aarch64-linux" => PackageBuildSystem::Aarch64Linux,
            "aarch64-macos" => PackageBuildSystem::Aarch64Macos,
            "x86_64-linux" => PackageBuildSystem::X8664Linux,
            "x86_64-macos" => PackageBuildSystem::X8664Macos,
            _ => PackageBuildSystem::default(),
        }
    }
}

pub fn get_build_system<T: BuildSystem>(system: &str) -> T {
    T::from_str(system)
}
