"""protoc-gen-go-grpc artifact (built from grpc-go source via the Go builder).

Mirrors ``sdk/typescript/src/artifact/protoc_gen_go_grpc.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import ArtifactSource

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class ProtocGenGoGrpc:
    """Builder for the protoc-gen-go-grpc artifact."""

    def build(self, context: ConfigContext) -> str:
        # Deferred import breaks the go-tool<->language-builder cycle.
        from vorpal_sdk.artifact.language.go import Go

        name = "protoc-gen-go-grpc"
        source_version = "1.79.1"
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"protoc-gen-go-grpc-v{source_version}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        build_directory = f"grpc-go-{source_version}/cmd/{name}"
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
            .with_source(source)
            .build(context)
        )
