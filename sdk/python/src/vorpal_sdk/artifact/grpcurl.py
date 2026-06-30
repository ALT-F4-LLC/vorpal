"""grpcurl artifact.

Mirrors ``sdk/rust/src/artifact/grpcurl.rs``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import ArtifactSource
from vorpal_sdk.artifact.language.go import Go
from vorpal_sdk.artifact.protoc import Protoc

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Grpcurl:
    """Builder for the grpcurl artifact."""

    def __init__(self) -> None:
        self._protoc: str | None = None

    def with_protoc(self, protoc: str) -> Grpcurl:
        self._protoc = protoc
        return self

    def build(self, context: ConfigContext) -> str:
        protoc = self._protoc if self._protoc is not None else Protoc().build(context)

        name = "grpcurl"

        source_version = "1.9.3"
        source_path = (
            f"https://sdk.vorpal.build/source/grpcurl-v{source_version}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        build_directory = f"{name}-{source_version}"
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
            .with_artifacts([protoc])
            .with_build_directory(build_directory)
            .with_build_path(build_path)
            .with_source(source)
            .build(context)
        )
