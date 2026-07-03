"""Node.js runtime artifact (official binary distribution).

Mirrors ``sdk/typescript/src/artifact/nodejs.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class NodeJS:
    """Builder for the Node.js runtime artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "nodejs"
        system = context.get_system()

        if system == artifact_pb2.AARCH64_DARWIN:
            source_target = "darwin-arm64"
        elif system == artifact_pb2.AARCH64_LINUX:
            source_target = "linux-arm64"
        elif system == artifact_pb2.X8664_DARWIN:
            source_target = "darwin-x64"
        elif system == artifact_pb2.X8664_LINUX:
            source_target = "linux-x64"
        else:
            raise ValueError(f"unsupported {name} system: {system}")

        source_version = "22.22.0"
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"node-v{source_version}-{source_target}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        step_script = (
            f'cp -pr "./source/{name}/node-v{source_version}-'
            f'{source_target}/." "$VORPAL_OUTPUT"'
        )
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
