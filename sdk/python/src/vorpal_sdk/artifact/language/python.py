"""Python project + development-environment artifact builders.

Mirrors ``sdk/typescript/src/artifact/language/python.js`` and the Go SDK's
``language/python.go`` (the digest authority). The build script must match the
Rust/Go/TS SDKs character-for-character for the same inputs.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import (
    Artifact,
    ArtifactSource,
    DevelopmentEnvironment,
    get_env_key,
    secrets_to_proto,
)
from vorpal_sdk.artifact.cpython import Cpython
from vorpal_sdk.artifact.uv import Uv
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext

# Reproducible-build timestamp for `uv build` wheels. Wheels are zip archives, and
# the zip format cannot represent dates before 1980 — so SOURCE_DATE_EPOCH=0 would
# yield an invalid wheel. This is the zip epoch (1980-01-01T00:00:00Z).
SOURCE_DATE_EPOCH = "315532800"


def step_build_command(
    name: str, entrypoint: str | None, cpython_bin: str
) -> str:
    """Compose the mode-specific portion of the build step script.

    App mode (``entrypoint`` set) emits a relocatable launcher at
    ``$VORPAL_OUTPUT/bin/<name>`` that forwards its argv to the entrypoint. The
    interpreter store path (``cpython_bin``) is baked at build time, so the UNQUOTED
    heredoc (``<< EOF``) writes it literally while runtime vars are escaped as ``\\$``
    to be written without expansion during the build step. A quoted heredoc would
    suppress the baked interpreter path, so it MUST stay unquoted.

    Library mode (no entrypoint) runs ``uv build`` and copies the wheel/sdist,
    ``pyproject.toml``, and ``uv.lock`` to ``$VORPAL_OUTPUT/``.
    """
    if entrypoint is not None:
        return f"""cp -pr . "$VORPAL_OUTPUT/"

mkdir -p "$VORPAL_OUTPUT/bin"

cat > "$VORPAL_OUTPUT/bin/{name}" << EOF
#!/usr/bin/env bash
set -euo pipefail
VORPAL_PYTHON_ROOT="\\$(cd "\\$(dirname "\\${{BASH_SOURCE[0]}}")/.." && pwd)"
PYTHONPATH_EXTRA="\\$VORPAL_PYTHON_ROOT"
for site in "\\$VORPAL_PYTHON_ROOT"/.venv/lib/python*/site-packages; do
    [ -d "\\$site" ] && PYTHONPATH_EXTRA="\\$site:\\$PYTHONPATH_EXTRA"
done
export PYTHONPATH="\\$PYTHONPATH_EXTRA\\${{PYTHONPATH:+:\\$PYTHONPATH}}"
exec "{cpython_bin}/python3" "\\$VORPAL_PYTHON_ROOT/{entrypoint}" "\\$@"
EOF

chmod +x "$VORPAL_OUTPUT/bin/{name}\""""

    return """uv build

mkdir -p "$VORPAL_OUTPUT"

cp -pr dist/. "$VORPAL_OUTPUT/"
cp pyproject.toml "$VORPAL_OUTPUT/"
cp uv.lock "$VORPAL_OUTPUT/\""""


class Python:
    """Builder for Python project artifacts.

    ``uv sync --frozen`` is the hash-enforcement surface: uv verifies every package
    against the per-package SHA-256 in the committed ``uv.lock`` and fails closed on a
    content-hash mismatch. ``UV_PYTHON_DOWNLOADS=never`` + ``UV_PYTHON`` pinned to the
    Vorpal interpreter guarantee uv never fetches an interpreter at build time.
    """

    def __init__(
        self, name: str, systems: list[artifact_pb2.ArtifactSystem]
    ) -> None:
        self._aliases: list[str] = []
        self._artifacts: list[str] = []
        self._entrypoint: str | None = None
        self._environments: list[str] = []
        self._includes: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._source_scripts: list[str] = []
        self._systems = systems
        self._working_dir: str | None = None

    def with_aliases(self, aliases: list[str]) -> Python:
        self._aliases = aliases
        return self

    def with_artifacts(self, artifacts: list[str]) -> Python:
        self._artifacts = artifacts
        return self

    def with_entrypoint(self, entrypoint: str) -> Python:
        self._entrypoint = entrypoint
        return self

    def with_environments(self, environments: list[str]) -> Python:
        self._environments = environments
        return self

    def with_includes(self, includes: list[str]) -> Python:
        self._includes = includes
        return self

    def with_secrets(self, secrets: dict[str, str]) -> Python:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def with_source_scripts(self, scripts: list[str]) -> Python:
        for script in scripts:
            if script not in self._source_scripts:
                self._source_scripts.append(script)
        return self

    def with_working_dir(self, directory: str) -> Python:
        self._working_dir = directory
        return self

    def build(self, context: ConfigContext) -> str:
        cpython_digest = Cpython().build(context)
        cpython_bin = f"{get_env_key(cpython_digest)}/bin"

        uv_digest = Uv().build(context)
        uv_bin = f"{get_env_key(uv_digest)}/bin"

        source_path = "."
        source_builder = ArtifactSource(self._name, source_path)
        if len(self._includes) > 0:
            source_builder.with_includes(self._includes)
        source = source_builder.build()

        step_source_dir = f"{source_path}/source/{source.name}"
        if self._working_dir is not None:
            step_source_dir = f"{step_source_dir}/{self._working_dir}"

        # TRUST: name/entrypoint/working_dir are interpolated unescaped into the build
        # shell — CONFIG-AUTHOR-CONTROLLED (workspace trust, same as source scripts).
        step_build_cmd = step_build_command(
            self._name, self._entrypoint, cpython_bin
        )

        source_scripts = "\n".join(self._source_scripts)
        step_script = (
            f"pushd {step_source_dir}\n\n"
            f"{source_scripts}\n\n"
            f"uv sync --frozen --no-dev\n\n"
            f"{step_build_cmd}"
        )

        step_environments = [
            f"PATH={uv_bin}:{cpython_bin}",
            f"UV_PYTHON={cpython_bin}/python3",
            "UV_PYTHON_DOWNLOADS=never",
            "UV_LINK_MODE=copy",
            "UV_CACHE_DIR=$VORPAL_WORKSPACE/uv/cache",
            f"SOURCE_DATE_EPOCH={SOURCE_DATE_EPOCH}",
            *self._environments,
        ]

        step_artifacts = [cpython_digest, uv_digest, *self._artifacts]

        step = shell(
            context,
            step_artifacts,
            step_environments,
            step_script,
            secrets_to_proto(self._secrets),
        )

        return (
            Artifact(self._name, [step], self._systems)
            .with_aliases(self._aliases)
            .with_sources([source])
            .build(context)
        )


class PythonDevelopmentEnvironment:
    """Builder for Python development-environment artifacts.

    Pins ``UV_PYTHON`` to the Vorpal-managed interpreter and sets
    ``UV_PYTHON_DOWNLOADS=never`` so the dev shell never fetches an interpreter.
    """

    def __init__(
        self, name: str, systems: list[artifact_pb2.ArtifactSystem]
    ) -> None:
        self._artifacts: list[str] = []
        self._environments: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._systems = systems

    def with_artifacts(
        self, artifacts: list[str]
    ) -> PythonDevelopmentEnvironment:
        self._artifacts.extend(artifacts)
        return self

    def with_environments(
        self, environments: list[str]
    ) -> PythonDevelopmentEnvironment:
        self._environments.extend(environments)
        return self

    def with_secrets(
        self, secrets: dict[str, str]
    ) -> PythonDevelopmentEnvironment:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def build(self, context: ConfigContext) -> str:
        cpython = Cpython().build(context)
        cpython_bin = f"{get_env_key(cpython)}/bin"

        uv = Uv().build(context)

        artifacts = [cpython, uv, *self._artifacts]

        # Pin the dev-shell interpreter and suppress uv's auto-download so the shell
        # always uses the Vorpal-managed CPython (Go/Rust env-var pattern).
        environments = [
            f"UV_PYTHON={cpython_bin}/python3",
            "UV_PYTHON_DOWNLOADS=never",
            *self._environments,
        ]

        devenv = (
            DevelopmentEnvironment(self._name, self._systems)
            .with_artifacts(artifacts)
            .with_environments(environments)
        )

        if len(self._secrets) > 0:
            devenv = devenv.with_secrets(self._secrets)

        return devenv.build(context)
