use crate::{
    api,
    api::artifact::ArtifactSystem,
    artifact::{
        cpython::Cpython, get_env_key, step, uv::Uv, Artifact, ArtifactSource,
        DevelopmentEnvironment,
    },
    context::ConfigContext,
};
use anyhow::Result;
use indoc::formatdoc;

/// Reproducible-build timestamp for `uv build` wheels. Wheels are zip archives, and the
/// zip format cannot represent dates before 1980 — so `SOURCE_DATE_EPOCH=0` would yield an
/// invalid wheel. This is the zip epoch (1980-01-01T00:00:00Z).
const SOURCE_DATE_EPOCH: &str = "315532800";

pub struct Python<'a> {
    aliases: Vec<String>,
    artifacts: Vec<String>,
    entrypoint: Option<&'a str>,
    environments: Vec<&'a str>,
    name: &'a str,
    secrets: Vec<api::artifact::ArtifactStepSecret>,
    source_includes: Vec<&'a str>,
    source_scripts: Vec<String>,
    systems: Vec<ArtifactSystem>,
    working_dir: Option<String>,
}

/// Composes the mode-specific portion of the build step script.
///
/// App mode (`entrypoint` set) emits a relocatable launcher at `$VORPAL_OUTPUT/bin/<name>`
/// that forwards its argv (`exec … "$@"`) to the entrypoint, so the CLI run contract
/// (`<bin> start --agent … --port … --registry …` → prints `context service:`) reaches the
/// Python entrypoint. The launcher resolves its own root at runtime via `BASH_SOURCE` (not a
/// build-time absolute), but execs the pinned interpreter by its content-addressed store path
/// (`python_bin`, baked here) — that path is permanent, and the per-artifact `VORPAL_ARTIFACT_*`
/// env var is NOT set when the launcher runs as a transitive dependency, so a literal env
/// reference would break. PYTHONPATH points at the copied venv site-packages + project root,
/// avoiding a baked-shebang venv (which would break relocation).
///
/// `cp -pr .` copies the includes-filtered working tree (controlled by `with_includes` on the
/// builder) together with the uv-synced `.venv` into the world-readable content-addressed
/// store. Config authors should set `with_includes` to constrain what is baked — without it,
/// the entire project directory (`.env` files, credentials, generated secrets) may land in
/// the store.
///
/// Library mode (no entrypoint) builds a wheel via `uv build` and copies the wheel/sdist,
/// `pyproject.toml`, and `uv.lock` to `$VORPAL_OUTPUT/`.
fn step_build_command(name: &str, entrypoint: Option<&str>, python_bin: &str) -> String {
    match entrypoint {
        Some(entrypoint) => formatdoc! {r#"
            cp -pr . "$VORPAL_OUTPUT/"

            mkdir -p "$VORPAL_OUTPUT/bin"

            cat > "$VORPAL_OUTPUT/bin/{name}" << EOF
            #!/usr/bin/env bash
            set -euo pipefail
            VORPAL_PYTHON_ROOT="\$(cd "\$(dirname "\${{BASH_SOURCE[0]}}")/.." && pwd)"
            PYTHONPATH_EXTRA="\$VORPAL_PYTHON_ROOT"
            for site in "\$VORPAL_PYTHON_ROOT"/.venv/lib/python*/site-packages; do
                [ -d "\$site" ] && PYTHONPATH_EXTRA="\$site:\$PYTHONPATH_EXTRA"
            done
            export PYTHONPATH="\$PYTHONPATH_EXTRA\${{PYTHONPATH:+:\$PYTHONPATH}}"
            exec "{python_bin}/python3" "\$VORPAL_PYTHON_ROOT/{entrypoint}" "\$@"
            EOF

            chmod +x "$VORPAL_OUTPUT/bin/{name}""#,
        },
        None => formatdoc! {r#"
            uv build

            mkdir -p "$VORPAL_OUTPUT"

            cp -pr dist/. "$VORPAL_OUTPUT/"
            cp pyproject.toml "$VORPAL_OUTPUT/"
            cp uv.lock "$VORPAL_OUTPUT/""#,
        },
    }
}

impl<'a> Python<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            aliases: vec![],
            artifacts: vec![],
            entrypoint: None,
            environments: vec![],
            name,
            secrets: vec![],
            source_includes: vec![],
            source_scripts: vec![],
            systems,
            working_dir: None,
        }
    }

    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        for alias in aliases {
            if !self.aliases.contains(&alias) {
                self.aliases.push(alias);
            }
        }
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_entrypoint(mut self, entrypoint: &'a str) -> Self {
        self.entrypoint = Some(entrypoint);
        self
    }

    pub fn with_environments(mut self, environments: Vec<&'a str>) -> Self {
        self.environments = environments;
        self
    }

    pub fn with_includes(mut self, includes: Vec<&'a str>) -> Self {
        self.source_includes = includes;
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(String, String)>) -> Self {
        for (name, value) in secrets {
            if !self.secrets.iter().any(|s| s.name == name) {
                self.secrets
                    .push(api::artifact::ArtifactStepSecret { name, value });
            }
        }

        self
    }

    pub fn with_source_scripts(mut self, scripts: Vec<String>) -> Self {
        for script in scripts {
            if !self.source_scripts.contains(&script) {
                self.source_scripts.push(script);
            }
        }
        self
    }

    pub fn with_working_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    pub async fn build(mut self, context: &mut ConfigContext) -> Result<String> {
        // Setup toolchain artifacts

        let cpython = Cpython::new().build(context).await?;
        let cpython_bin = format!("{}/bin", get_env_key(&cpython));

        let uv = Uv::new().build(context).await?;
        let uv_bin = format!("{}/bin", get_env_key(&uv));

        // Setup source

        let source_path = ".";

        let mut source_builder = ArtifactSource::new(self.name, source_path);

        if !self.source_includes.is_empty() {
            source_builder = source_builder
                .with_includes(self.source_includes.iter().map(|s| s.to_string()).collect());
        }

        let source = source_builder.build();

        // Setup step source directory

        let step_source_dir = format!("{}/source/{}", source_path, source.name);

        let step_source_dir = match &self.working_dir {
            Some(working_dir) => format!("{}/{}", step_source_dir, working_dir),
            None => step_source_dir.clone(),
        };

        // Build step script
        //
        // `uv sync --frozen` is THE hash-enforcement surface: uv verifies every package
        // against the per-package SHA-256 in the committed `uv.lock` and fails closed on a
        // content-hash mismatch (there is no `uv sync --require-hashes` flag — that is uv's
        // pip-interface flag). `UV_PYTHON_DOWNLOADS=never` + `UV_PYTHON` pinned to the Vorpal
        // interpreter guarantee uv never fetches an interpreter at build time.

        // TRUST: `name`, `entrypoint`, and `working_dir` are interpolated unescaped into the
        // build shell — CONFIG-AUTHOR-CONTROLLED (workspace trust, same as with_source_scripts).
        // Not for untrusted or registry-derived input; see go.rs / typescript.rs for precedent.
        let step_build_command = step_build_command(self.name, self.entrypoint, &cpython_bin);

        let step_script = formatdoc! {r#"
            pushd {step_source_dir}

            {step_source_scripts}

            uv sync --frozen --no-dev --no-editable

            {step_build_command}"#,
            step_source_scripts = self.source_scripts.join("\n")
        };

        let mut step_environments = vec![
            format!("PATH={uv_bin}:{cpython_bin}"),
            format!("UV_PYTHON={cpython_bin}/python3"),
            "UV_PYTHON_DOWNLOADS=never".to_string(),
            "UV_LINK_MODE=copy".to_string(),
            "UV_CACHE_DIR=$VORPAL_WORKSPACE/uv/cache".to_string(),
            format!("SOURCE_DATE_EPOCH={SOURCE_DATE_EPOCH}"),
        ];

        for env in self.environments {
            step_environments.push(env.to_string());
        }

        let mut step_artifacts = vec![cpython.clone(), uv.clone()];

        step_artifacts.extend(self.artifacts);

        // Sort for deterministic output

        self.secrets.sort_by(|a, b| a.name.cmp(&b.name));

        let steps = vec![
            step::shell(
                context,
                step_artifacts,
                step_environments,
                step_script,
                self.secrets,
            )
            .await?,
        ];

        Artifact::new(self.name, steps, self.systems)
            .with_aliases(self.aliases)
            .with_sources(vec![source])
            .build(context)
            .await
    }
}

pub struct PythonDevelopmentEnvironment<'a> {
    artifacts: Vec<String>,
    environments: Vec<String>,
    name: &'a str,
    secrets: Vec<(&'a str, &'a str)>,
    systems: Vec<ArtifactSystem>,
}

impl<'a> PythonDevelopmentEnvironment<'a> {
    pub fn new(name: &'a str, systems: Vec<ArtifactSystem>) -> Self {
        Self {
            artifacts: vec![],
            environments: vec![],
            name,
            secrets: vec![],
            systems,
        }
    }

    pub fn with_artifacts(mut self, artifacts: Vec<String>) -> Self {
        self.artifacts.extend(artifacts);
        self
    }

    pub fn with_environments(mut self, environments: Vec<String>) -> Self {
        self.environments.extend(environments);
        self
    }

    pub fn with_secrets(mut self, secrets: Vec<(&'a str, &'a str)>) -> Self {
        for secret in secrets {
            if !self.secrets.iter().any(|(name, _)| *name == secret.0) {
                self.secrets.push(secret);
            }
        }
        self
    }

    pub async fn build(self, context: &mut ConfigContext) -> Result<String> {
        let cpython = Cpython::new().build(context).await?;
        let cpython_bin = format!("{}/bin", get_env_key(&cpython));

        let uv = Uv::new().build(context).await?;

        let mut artifacts = vec![cpython, uv];
        artifacts.extend(self.artifacts);

        // Pin the dev-shell interpreter and suppress uv's auto-download so the
        // shell always uses the Vorpal-managed CPython (Go/Rust env-var pattern).
        let mut environments = vec![
            format!("UV_PYTHON={cpython_bin}/python3"),
            "UV_PYTHON_DOWNLOADS=never".to_string(),
        ];

        environments.extend(self.environments);

        let mut devenv = DevelopmentEnvironment::new(self.name, self.systems)
            .with_artifacts(artifacts)
            .with_environments(environments);

        if !self.secrets.is_empty() {
            devenv = devenv.with_secrets(self.secrets);
        }

        devenv.build(context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_mode_emits_argv_forwarding_launcher() {
        let script =
            step_build_command("example", Some("src/example.py"), "$VORPAL_ARTIFACT_PY/bin");

        // Launcher lands at the parity output path. The interpreter store path is baked at
        // build time (unescaped `$VORPAL_ARTIFACT_PY`); the launcher's runtime vars stay
        // backslash-escaped here so the build-step heredoc writes them literally.
        assert!(script.contains(r#""$VORPAL_OUTPUT/bin/example""#));
        assert!(script.contains(
            r#"exec "$VORPAL_ARTIFACT_PY/bin/python3" "\$VORPAL_PYTHON_ROOT/src/example.py" "\$@""#
        ));
        // App mode does not build a wheel.
        assert!(!script.contains("uv build"));
    }

    #[test]
    fn library_mode_emits_wheel_and_lock() {
        let script = step_build_command("example", None, "$VORPAL_ARTIFACT_PY/bin");

        assert!(script.contains("uv build"));
        assert!(script.contains(r#"cp -pr dist/. "$VORPAL_OUTPUT/""#));
        assert!(script.contains(r#"cp uv.lock "$VORPAL_OUTPUT/""#));
        // Library mode emits no launcher.
        assert!(!script.contains("/bin/example"));
    }
}
