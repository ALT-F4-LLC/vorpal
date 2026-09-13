"""crane artifact (built from go-containerregistry source via the Go builder).

Mirrors ``sdk/rust/src/artifact/crane.rs`` and ``sdk/typescript/src/artifact/crane.ts``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.artifact import ArtifactSource
from vorpal_sdk.artifact.language.go import Go
from vorpal_sdk.system import SYSTEMS

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Crane:
    """Builder for the crane artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "crane"
        version = "0.21.1"

        source_path = f"https://sdk.vorpal.build/source/crane-v{version}.tar.gz"
        source = ArtifactSource(name, source_path).build()

        build_directory = f"./go-containerregistry-{version}"
        build_path = f"./cmd/{name}"


        return (
            Go(name, SYSTEMS)
            .with_aliases([f"{name}:{version}"])
            .with_build_directory(build_directory)
            .with_build_path(build_path)
            .with_source(source)
            .build(context)
        )
