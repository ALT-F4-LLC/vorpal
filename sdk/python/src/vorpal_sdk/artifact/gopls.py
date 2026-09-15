"""gopls artifact (built from the Go tools source via the Go builder).

Mirrors ``sdk/typescript/src/artifact/gopls.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.artifact.go import source_tools
from vorpal_sdk.system import SYSTEMS

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Gopls:
    """Builder for the gopls artifact."""

    def build(self, context: ConfigContext) -> str:
        # Deferred import breaks the go-tool<->language-builder cycle.
        from vorpal_sdk.artifact.language.go import Go

        name = "gopls"

        return (
            Go(name, SYSTEMS)
            .with_aliases([f"{name}:0.42.0"])
            .with_build_directory(name)
            .with_source(source_tools(name))
            .build(context)
        )
