"""staticcheck artifact (built from the go-tools source via the Go builder).

Mirrors ``sdk/typescript/src/artifact/staticcheck.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import ArtifactSource

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Staticcheck:
    """Builder for the staticcheck artifact."""

    def build(self, context: ConfigContext) -> str:
        # Deferred import breaks the go-tool<->language-builder cycle.
        from vorpal_sdk.artifact.language.go import Go

        name = "staticcheck"
        source_version = "2026.1"
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"staticcheck-{source_version}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        build_directory = f"go-tools-{source_version}"
        build_path = f"cmd/{name}/{name}.go"
        systems = [
            artifact_pb2.AARCH64_DARWIN,
            artifact_pb2.AARCH64_LINUX,
            artifact_pb2.X8664_DARWIN,
            artifact_pb2.X8664_LINUX,
        ]

        return (
            Go(name, systems)
            .with_aliases([f"{name}:{source_version}"])
            .with_build_directory(build_directory)
            .with_build_path(build_path)
            .with_source(source)
            .build(context)
        )
