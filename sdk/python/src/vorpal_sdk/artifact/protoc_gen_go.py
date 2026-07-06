"""protoc-gen-go artifact (prebuilt binary).

Mirrors ``sdk/typescript/src/artifact/protoc_gen_go.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class ProtocGenGo:
    """Builder for the protoc-gen-go artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "protoc-gen-go"
        system = context.get_system()

        if system == artifact_pb2.AARCH64_DARWIN:
            source_target = "darwin.arm64"
        elif system == artifact_pb2.AARCH64_LINUX:
            source_target = "linux.arm64"
        elif system == artifact_pb2.X8664_DARWIN:
            source_target = "darwin.amd64"
        elif system == artifact_pb2.X8664_LINUX:
            source_target = "linux.amd64"
        else:
            raise ValueError(f"unsupported {name} system: {system}")

        source_version = "1.36.11"
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"protoc-gen-go.v{source_version}.{source_target}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        step_script = """mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/protoc-gen-go/protoc-gen-go" "$VORPAL_OUTPUT/bin/protoc-gen-go"

chmod +x "$VORPAL_OUTPUT/bin/protoc-gen-go\""""

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
