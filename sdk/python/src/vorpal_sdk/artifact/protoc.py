"""protoc (Protocol Buffers compiler) artifact.

Mirrors ``sdk/typescript/src/artifact/protoc.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Protoc:
    """Builder for the protoc artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "protoc"
        system = context.get_system()

        if system == artifact_pb2.AARCH64_DARWIN:
            source_target = "osx-aarch_64"
        elif system == artifact_pb2.AARCH64_LINUX:
            source_target = "linux-aarch_64"
        elif system == artifact_pb2.X8664_DARWIN:
            source_target = "osx-x86_64"
        elif system == artifact_pb2.X8664_LINUX:
            source_target = "linux-x86_64"
        else:
            raise ValueError(f"unsupported {name} system: {system}")

        source_version = "34.0"
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"protoc-{source_version}-{source_target}.zip"
        )

        source = ArtifactSource(name, source_path).build()

        step_script = f"""mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/{name}/bin/protoc" "$VORPAL_OUTPUT/bin/protoc"

chmod +x "$VORPAL_OUTPUT/bin/protoc\""""

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
