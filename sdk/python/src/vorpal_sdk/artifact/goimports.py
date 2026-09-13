"""goimports artifact (built from the Go tools source via the Go builder).

Mirrors ``sdk/typescript/src/artifact/goimports.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.artifact.go import source_tools
from vorpal_sdk.system import SYSTEMS

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Goimports:
    """Builder for the goimports artifact."""

    def build(self, context: ConfigContext) -> str:
        # Deferred import breaks the go-tool<->language-builder cycle (language/go
        # imports this module via its DevelopmentEnvironment).
        from vorpal_sdk.artifact.language.go import Go

        name = "goimports"
        build_directory = f"cmd/{name}"

        return (
            Go(name, SYSTEMS)
            .with_aliases([f"{name}:0.42.0"])
            .with_build_directory(build_directory)
            .with_source(source_tools(name))
            .build(context)
        )
