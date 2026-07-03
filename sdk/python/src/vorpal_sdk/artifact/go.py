"""Go distribution + shared Go-tools source helpers.

Mirrors ``sdk/typescript/src/artifact/go.js`` (``GoBin`` + ``sourceTools``).
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


def source_tools(name: str) -> artifact_pb2.ArtifactSource:
    """Build the shared ArtifactSource for the Go tools repository.

    Used by goimports and gopls, which both build from the same source.
    Mirrors Rust ``go::source_tools()``.
    """
    version = "0.42.0"
    path = f"https://sdk.vorpal.build/source/go-tools-v{version}.tar.gz"
    return ArtifactSource(name, path).build()


class GoBin:
    """Builder for the Go distribution artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "go"
        system = context.get_system()

        if system == artifact_pb2.AARCH64_DARWIN:
            source_target = "darwin-arm64"
        elif system == artifact_pb2.AARCH64_LINUX:
            source_target = "linux-arm64"
        elif system == artifact_pb2.X8664_DARWIN:
            source_target = "darwin-amd64"
        elif system == artifact_pb2.X8664_LINUX:
            source_target = "linux-amd64"
        else:
            raise ValueError(f"unsupported {name} system: {system}")

        source_version = "1.26.0"
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"go{source_version}.{source_target}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        step_script = f'cp -pr "./source/{name}/go/." "$VORPAL_OUTPUT"'
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
