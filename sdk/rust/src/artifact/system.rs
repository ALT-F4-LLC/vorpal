use crate::api::artifact::{
    ArtifactSystem,
    ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
};
use anyhow::Result;
use std::env::consts::{ARCH, OS};

pub trait ArtifactSystemInput {
    fn into_artifact_system(self) -> Result<ArtifactSystem>;
}

impl ArtifactSystemInput for ArtifactSystem {
    fn into_artifact_system(self) -> Result<ArtifactSystem> {
        match self {
            ArtifactSystem::UnknownSystem => {
                Err(anyhow::anyhow!("unsupported system: UNKNOWN_SYSTEM"))
            }
            _ => Ok(self),
        }
    }
}

impl ArtifactSystemInput for &ArtifactSystem {
    fn into_artifact_system(self) -> Result<ArtifactSystem> {
        (*self).into_artifact_system()
    }
}

impl ArtifactSystemInput for String {
    fn into_artifact_system(self) -> Result<ArtifactSystem> {
        get_system(&self)
    }
}

impl<T> ArtifactSystemInput for &T
where
    T: AsRef<str> + ?Sized,
{
    fn into_artifact_system(self) -> Result<ArtifactSystem> {
        get_system(self.as_ref())
    }
}

pub fn get_system_default_str() -> String {
    let os = match OS {
        "macos" => "darwin",
        _ => OS,
    };

    format!("{ARCH}-{os}")
}

pub fn get_system_default() -> Result<ArtifactSystem> {
    let platform = get_system_default_str();

    get_system(&platform)
}

pub fn get_system(system: &str) -> Result<ArtifactSystem> {
    match system {
        "aarch64-darwin" => Ok(Aarch64Darwin),
        "aarch64-linux" => Ok(Aarch64Linux),
        "x86_64-darwin" => Ok(X8664Darwin),
        "x86_64-linux" => Ok(X8664Linux),
        _ => Err(anyhow::anyhow!("unsupported system: {}", system)),
    }
}

pub fn normalize_systems<I, S>(systems: I) -> Result<Vec<ArtifactSystem>>
where
    I: IntoIterator<Item = S>,
    S: ArtifactSystemInput,
{
    systems
        .into_iter()
        .map(|system| system.into_artifact_system())
        .collect()
}

pub fn get_systems<I, S>(systems: I) -> Result<Vec<ArtifactSystem>>
where
    I: IntoIterator<Item = S>,
    S: ArtifactSystemInput,
{
    normalize_systems(systems)
}

pub(crate) fn normalize_systems_for_builder<I, S>(
    systems: I,
) -> (Vec<ArtifactSystem>, Option<anyhow::Error>)
where
    I: IntoIterator<Item = S>,
    S: ArtifactSystemInput,
{
    match normalize_systems(systems) {
        Ok(systems) => (systems, None),
        Err(error) => (vec![], Some(error)),
    }
}

pub(crate) fn check_system_error(system_error: &mut Option<anyhow::Error>) -> Result<()> {
    if let Some(error) = system_error.take() {
        return Err(error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_system_accepts_canonical_strings() -> Result<()> {
        assert_eq!(get_system("aarch64-darwin")?, Aarch64Darwin);
        assert_eq!(get_system("aarch64-linux")?, Aarch64Linux);
        assert_eq!(get_system("x86_64-darwin")?, X8664Darwin);
        assert_eq!(get_system("x86_64-linux")?, X8664Linux);

        Ok(())
    }

    #[test]
    fn get_system_rejects_enum_labels() {
        for system in [
            "AARCH64_DARWIN",
            "AARCH64_LINUX",
            "X8664_DARWIN",
            "X8664_LINUX",
        ] {
            let err = get_system(system).unwrap_err();

            assert_eq!(err.to_string(), format!("unsupported system: {system}"));
        }
    }

    #[test]
    fn normalize_systems_accepts_str_inputs_and_preserves_order() -> Result<()> {
        let systems = normalize_systems(["x86_64-linux", "aarch64-darwin", "aarch64-linux"])?;

        assert_eq!(systems, vec![X8664Linux, Aarch64Darwin, Aarch64Linux]);

        Ok(())
    }

    #[test]
    fn normalize_systems_accepts_string_inputs() -> Result<()> {
        let systems = normalize_systems(vec![
            "aarch64-darwin".to_string(),
            "x86_64-linux".to_string(),
        ])?;

        assert_eq!(systems, vec![Aarch64Darwin, X8664Linux]);

        Ok(())
    }

    #[test]
    fn normalize_systems_accepts_artifact_system_inputs() -> Result<()> {
        let systems = normalize_systems(vec![X8664Linux, Aarch64Linux])?;

        assert_eq!(systems, vec![X8664Linux, Aarch64Linux]);

        Ok(())
    }

    #[test]
    fn normalize_systems_rejects_unknown_system_enum() {
        let err = ArtifactSystem::UnknownSystem
            .into_artifact_system()
            .unwrap_err();

        assert_eq!(err.to_string(), "unsupported system: UNKNOWN_SYSTEM");

        let system = ArtifactSystem::UnknownSystem;
        let err = (&system).into_artifact_system().unwrap_err();

        assert_eq!(err.to_string(), "unsupported system: UNKNOWN_SYSTEM");
    }

    #[test]
    fn get_systems_wraps_normalize_systems() -> Result<()> {
        let systems = get_systems(["x86_64-linux", "aarch64-darwin"])?;

        assert_eq!(systems, vec![X8664Linux, Aarch64Darwin]);

        Ok(())
    }

    #[test]
    fn get_system_rejects_invalid_strings() {
        let err = get_system("loongarch64-linux").unwrap_err();

        assert_eq!(err.to_string(), "unsupported system: loongarch64-linux");
    }

    #[test]
    fn normalize_systems_rejects_sentinel_string_labels() {
        let err = normalize_systems(["UNKNOWN_SYSTEM"]).unwrap_err();

        assert_eq!(err.to_string(), "unsupported system: UNKNOWN_SYSTEM");
    }
}
