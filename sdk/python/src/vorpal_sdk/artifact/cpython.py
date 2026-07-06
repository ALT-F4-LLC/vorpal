"""CPython interpreter artifact (python-build-standalone, relocatable).

Mirrors ``sdk/typescript/src/artifact/cpython.js``.

Source name is ``cpython`` (NOT ``python``) — the linux_vorpal bootstrap already
owns a ``python`` source compiled from source, and sources key by (name, platform),
so reusing ``python`` would collide (ADR 0001 / TDD §4, H1).

PROVENANCE — no inline digest (ADR 0001 Part A). The canonical pin is the per-triple
``Vorpal.lock`` entry captured via ``--unlock``; until that lands the HTTP source is
intentionally unpinned (the C1 mint gate fails closed with "unpinned - use --unlock").
A placeholder digest is intentionally avoided — the agent resolves an inline digest
against the registry cache without verifying content, so a predictable placeholder is
a pre-seedable cache-poison key.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext

# Canonical conforming copy of the build-target CPython interpreter pin. Operator rule:
# CPython 3.13, latest patch. Concrete pin: 3.13.14 (python-build-standalone tag
# 20260623, install_only). Byte-equal to the Rust canonical (ADR 0001 Part B).
DEFAULT_PYTHON_VERSION = "3.13.14"


def cpython_target(system: artifact_pb2.ArtifactSystem) -> str:
    """Map a Vorpal ArtifactSystem to the python-build-standalone target triple.

    Mirrors Rust ``cpython::target()`` — name-agnostic error on unknown system.
    """
    if system == artifact_pb2.AARCH64_DARWIN:
        return "aarch64-apple-darwin"
    if system == artifact_pb2.AARCH64_LINUX:
        return "aarch64-unknown-linux-gnu"
    if system == artifact_pb2.X8664_DARWIN:
        return "x86_64-apple-darwin"
    if system == artifact_pb2.X8664_LINUX:
        return "x86_64-unknown-linux-gnu"
    raise ValueError(f"unsupported toolchain target system: {system}")


class Cpython:
    """Builder for the CPython interpreter artifact."""

    def __init__(self) -> None:
        self._version = DEFAULT_PYTHON_VERSION

    def with_version(self, version: str) -> Cpython:
        self._version = version
        return self

    def build(self, context: ConfigContext) -> str:
        name = "cpython"
        system = context.get_system()

        source_target = cpython_target(system)
        source_version = self._version
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"cpython-{source_version}-{source_target}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        # pbs install_only tarballs unpack to a top-level python/ dir.
        step_script = f"""mkdir -p "$VORPAL_OUTPUT"
cp -prf "./source/{name}/python/." "$VORPAL_OUTPUT/"
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
