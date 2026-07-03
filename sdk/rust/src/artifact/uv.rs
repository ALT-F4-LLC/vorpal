use crate::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{cpython, step, Artifact, ArtifactSource},
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

/// Canonical pin for the build-target `uv` toolchain (Astral release).
pub const DEFAULT_UV_VERSION: &str = "0.10.11";

/// Build-target `uv` toolchain (Astral standalone release).
///
/// HASH-ENFORCEMENT (C3 foundation): on `uv sync --frozen`, uv-0.10.11 verifies each
/// package against the per-package hashes carried in `uv.lock` and rejects any content-hash
/// mismatch. That hashed-lock verification IS the require-hashes enforcement surface the
/// build helpers (DKT-7/8/9) wire — there is no `uv sync --require-hashes` CLI flag
/// (`--require-hashes` is uv's *pip-interface* flag); the enforced OUTCOME (not a flag token)
/// is what the C3a tampered-package test proves live (TDD §Phase-2 ~L802-815).
///
/// PROVENANCE — no inline `with_digest` (ADR 0001 Part A). The canonical pin is the
/// per-triple `Vorpal.lock` entry captured via `--unlock`, mirroring bun/nodejs/cargo;
/// until then the HTTP source is unpinned and the C1 mint gate fails the build closed. A
/// placeholder digest is intentionally avoided: `agent.rs` returns an inline digest on a
/// registry-cache hit without verifying content, so a predictable placeholder is a
/// pre-seedable cache-poison key. uv's per-platform upstream SHA-256s (ADR 0001 link a) are
/// verified against Astral's published checksums at capture and recorded in the provenance
/// manifest. See `cpython::Cpython` for the full two-link rationale.
pub struct Uv {
    version: String,
}

impl Default for Uv {
    fn default() -> Self {
        Self {
            version: DEFAULT_UV_VERSION.to_string(),
        }
    }
}

impl Uv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let name = "uv";

        let system = context.get_system();
        let source_target = cpython::target(system)?;
        let source_version = &self.version;
        let source_path =
            format!("https://sdk.vorpal.build/source/uv-{source_version}-{source_target}.tar.gz");

        let source = ArtifactSource::new(name, source_path.as_str()).build();

        // Astral standalone release unpacks to uv-{triple}/uv at the tarball root.
        let step_script = formatdoc! {"
            mkdir -p \"$VORPAL_OUTPUT/bin\"
            cp -p \"./source/{name}/uv-{source_target}/uv\" \"$VORPAL_OUTPUT/bin/uv\"
            chmod +x \"$VORPAL_OUTPUT/bin/uv\"
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
