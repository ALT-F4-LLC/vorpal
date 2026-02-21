use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::{bail, Result};
use indoc::formatdoc;

/// Default Bun version pinned by Vorpal.
///
/// This is the single source of truth for the Bun runtime version used by the
/// TypeScript language builder. All TypeScript config builds use this version
/// unless explicitly overridden via `Bun::with_version`.
///
/// # Upgrade process
///
/// 1. Update this constant to the new Bun version.
/// 2. Run the full test suite to verify TypeScript config builds still work.
/// 3. Test on all supported platforms (aarch64-darwin, aarch64-linux, x86_64-darwin, x86_64-linux).
/// 4. Check the Bun changelog (<https://bun.sh/blog>) for breaking changes that
///    could affect `bun build --compile` or `bun install --frozen-lockfile`.
/// 5. Note the version bump in the Vorpal release notes.
///
/// # Breaking change handling
///
/// Bun is pre-1.0-stable in its compile/bundler behavior. When upgrading:
/// - Verify that `bun build --compile` still produces working standalone binaries.
/// - Verify that `bun install --frozen-lockfile` still resolves dependencies correctly.
/// - If a Bun release introduces breaking changes, pin to the last known-good version
///   and document the incompatibility.
pub const DEFAULT_BUN_VERSION: &str = "1.2.0";

/// Builder for the Bun runtime artifact.
///
/// By default, uses [`DEFAULT_BUN_VERSION`]. Callers can override the version
/// via [`Bun::with_version`] to support user-specified versions (e.g., from
/// `Vorpal.toml`).
pub struct Bun {
    version: String,
}

impl Default for Bun {
    fn default() -> Self {
        Self {
            version: DEFAULT_BUN_VERSION.to_string(),
        }
    }
}

impl Bun {
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the Bun version to use instead of [`DEFAULT_BUN_VERSION`].
    ///
    /// This enables user-specified Bun versions via `Vorpal.toml`:
    ///
    /// ```toml
    /// [source.typescript]
    /// bun_version = "1.2.1"
    /// ```
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "bun";

        let system = context.get_system();

        let source_target = match system {
            Aarch64Darwin => "darwin-aarch64",
            Aarch64Linux => "linux-aarch64",
            X8664Darwin => "darwin-x64",
            X8664Linux => "linux-x64",
            _ => bail!("unsupported {name} system: {}", system.as_str_name()),
        };

        let source_version = &self.version;
        let source_path = format!("https://github.com/oven-sh/bun/releases/download/bun-v{source_version}/bun-{source_target}.zip");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        let step_script = formatdoc! {"
            mkdir -pv \"$VORPAL_OUTPUT/bin\"
            cp -pv \"./source/{name}/bun-{source_target}/bun\" \"$VORPAL_OUTPUT/bin/bun\"
            chmod +x \"$VORPAL_OUTPUT/bin/bun\"
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
