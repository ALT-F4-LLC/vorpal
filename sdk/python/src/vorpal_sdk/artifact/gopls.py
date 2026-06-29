"""gopls artifact (built from the Go tools source via the Go builder).

Mirrors ``sdk/typescript/src/artifact/gopls.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact.go import source_tools

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Gopls:
    """Builder for the gopls artifact."""

    def build(self, context: ConfigContext) -> str:
        # Deferred import breaks the go-tool<->language-builder cycle.
        from vorpal_sdk.artifact.language.go import Go

        name = "gopls"
        systems = [
            artifact_pb2.AARCH64_DARWIN,
            artifact_pb2.AARCH64_LINUX,
            artifact_pb2.X8664_DARWIN,
            artifact_pb2.X8664_LINUX,
        ]

        return (
            Go(name, systems)
            .with_aliases([f"{name}:0.42.0"])
            .with_build_directory(name)
            .with_source(source_tools(name))
            .build(context)
        )
