use crate::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Canonical single source of truth for the build-target CPython interpreter pin.
///
/// Operator rule: CPython 3.13, latest patch. Concrete pin: 3.13.14 from
/// python-build-standalone release tag `20260623`, `install_only` (relocatable). Every
/// other Python-version pin in the repo — the Go/TS builder constants, `sdk/python`'s
/// `.python-version` and `requires-python` — is a conforming copy derived from THIS
/// constant, never an independent pin (ADR 0001 Part B).
pub const DEFAULT_PYTHON_VERSION: &str = "3.13.14";

/// Maps a Vorpal `ArtifactSystem` to the python-build-standalone target triple.
pub fn target(system: ArtifactSystem) -> Result<String> {
    let target = match system {
        Aarch64Darwin => "aarch64-apple-darwin",
        Aarch64Linux => "aarch64-unknown-linux-gnu",
        X8664Darwin => "x86_64-apple-darwin",
        X8664Linux => "x86_64-unknown-linux-gnu",
        _ => bail!(
            "unsupported toolchain target system: {}",
            system.as_str_name()
        ),
    };

    Ok(target.to_string())
}

/// Build-target CPython interpreter (python-build-standalone, relocatable `install_only`).
///
/// Source name is `cpython`, NOT `python` — the repo's existing linux_vorpal bootstrap
/// already owns a `python` source (compiled from source), and sources key by
/// `(name, platform)`, so reusing `python` would collide (ADR 0001 / TDD §4, H1).
///
/// PROVENANCE — no inline `with_digest` (ADR 0001 Part A). The canonical pin is the
/// per-triple `Vorpal.lock` entry captured via `--unlock`, mirroring bun/nodejs/cargo.
/// Until that capture lands, the HTTP source is intentionally unpinned: the C1 mint gate
/// fails the build closed with "unpinned - use --unlock", which is the true state. We do
/// NOT set a placeholder digest — `agent.rs` resolves an inline digest against the registry
/// cache (`check`) and returns it on a hit WITHOUT downloading or verifying content, so a
/// predictable placeholder would be a pre-seedable cache-poison key. The capture is
/// two-link (ADR 0001): (a) the mirror tarball's SHA-256 == the upstream pbs `SHA256SUMS`
/// fetched over TLS from the upstream origin (NOT sdk.vorpal.build); (b) `--unlock` computes
/// `get_source_digest` over the unpacked payload and records the per-triple digest in
/// `Vorpal.lock`. An inline `with_digest` may be added post-capture as defense-in-depth,
/// keyed per `(version, triple)` — and only then does the `with_digest` × `with_version`
/// footgun apply (an override against a single captured digest fails the build closed but
/// confusingly).
pub struct Cpython {
    version: String,
}

impl Default for Cpython {
    fn default() -> Self {
        Self {
            version: DEFAULT_PYTHON_VERSION.to_string(),
        }
    }
}

impl Cpython {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "cpython";

        let system = context.get_system();
        let source_target = target(system)?;
        let source_version = &self.version;
        let source_path = format!(
            "https://sdk.vorpal.build/source/cpython-{source_version}-{source_target}.tar.gz"
        );

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        // pbs install_only tarballs unpack to a top-level python/ dir at the tarball root.
        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT\"
            cp -prf \"./source/{name}/python/.\" \"$VORPAL_OUTPUT/\"
        "};

        let steps = vec![step::shell(context, vec![], vec![], step_script, vec![]).await?];
        let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

        Artifact::new(name, steps, systems)
            .with_aliases(vec![format!("{name}:{source_version}")])
            .with_sources(vec![source])
            .build(context)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::artifact::ArtifactSystem;

    #[test]
    fn target_maps_supported_systems() {
        assert_eq!(
            target(ArtifactSystem::Aarch64Darwin).unwrap(),
            "aarch64-apple-darwin"
        );
        assert_eq!(
            target(ArtifactSystem::Aarch64Linux).unwrap(),
            "aarch64-unknown-linux-gnu"
        );
        assert_eq!(
            target(ArtifactSystem::X8664Darwin).unwrap(),
            "x86_64-apple-darwin"
        );
        assert_eq!(
            target(ArtifactSystem::X8664Linux).unwrap(),
            "x86_64-unknown-linux-gnu"
        );
    }

    #[test]
    fn target_unknown_system_bails() {
        assert!(target(ArtifactSystem::UnknownSystem).is_err());
    }
}
