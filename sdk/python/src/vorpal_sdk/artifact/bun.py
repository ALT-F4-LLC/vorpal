"""Bun runtime artifact (prebuilt binary from a zip archive).

Mirrors ``sdk/typescript/src/artifact/bun.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext

DEFAULT_BUN_VERSION = "1.3.10"


class Bun:
    """Builder for the Bun runtime artifact."""

    def __init__(self) -> None:
        self._version = DEFAULT_BUN_VERSION

    def with_version(self, version: str) -> Bun:
        self._version = version
        return self

    def build(self, context: ConfigContext) -> str:
        name = "bun"
        system = context.get_system()

        if system == artifact_pb2.AARCH64_DARWIN:
            source_target = "darwin-aarch64"
        elif system == artifact_pb2.AARCH64_LINUX:
            source_target = "linux-aarch64"
        elif system == artifact_pb2.X8664_DARWIN:
            source_target = "darwin-x64"
        elif system == artifact_pb2.X8664_LINUX:
            source_target = "linux-x64-baseline"
        else:
            raise ValueError(f"unsupported {name} system: {system}")

        source_version = self._version
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"bun-{source_version}-{source_target}.zip"
        )

        source = ArtifactSource(name, source_path).build()

        step_script = f"""mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/{name}/bun-{source_target}/bun" "$VORPAL_OUTPUT/bin/bun"
chmod +x "$VORPAL_OUTPUT/bin/bun"
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
