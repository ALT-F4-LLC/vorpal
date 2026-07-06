"""uv toolchain artifact (Astral standalone release).

Mirrors ``sdk/typescript/src/artifact/uv.js``.

HASH-ENFORCEMENT (C3 foundation): on ``uv sync --frozen``, uv verifies each package
against the per-package hashes carried in ``uv.lock`` and rejects any content-hash
mismatch. That hashed-lock verification IS the require-hashes enforcement surface the
build helpers wire — there is no ``uv sync --require-hashes`` CLI flag.

PROVENANCE — no inline digest (ADR 0001 Part A); see ``cpython.py`` for the rationale.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.artifact.cpython import cpython_target
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext

# Canonical conforming copy of the build-target uv toolchain pin (Astral release).
# Byte-equal to the Rust canonical (ADR 0001 Part B).
DEFAULT_UV_VERSION = "0.10.11"


class Uv:
    """Builder for the uv toolchain artifact."""

    def __init__(self) -> None:
        self._version = DEFAULT_UV_VERSION

    def with_version(self, version: str) -> Uv:
        self._version = version
        return self

    def build(self, context: ConfigContext) -> str:
        name = "uv"
        system = context.get_system()

        source_target = cpython_target(system)
        source_version = self._version
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"uv-{source_version}-{source_target}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        # Astral standalone release unpacks to uv-{triple}/uv at the tarball root.
        step_script = f"""mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/{name}/uv-{source_target}/uv" "$VORPAL_OUTPUT/bin/uv"
chmod +x "$VORPAL_OUTPUT/bin/uv"
"""
        steps = [shell(context, [], [], step_script, [])]
        systems = [
            artifact_pb2.AARCH64_DARWIN,
            artifact_pb2.AARCH64_LINUX,
            artifact_pb2.X8664_DARWIN,
            artifact_pb2.X8664_LINUX,
        ]

        return (
            Artifact(name, steps, systems)
            .with_aliases([f"{name}:{source_version}"])
            .with_sources([source])
            .build(context)
        )
