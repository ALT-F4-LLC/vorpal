from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Rsync:
    """Builder for the rsync artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "rsync"
        version = "3.4.1"

        path = f"https://sdk.vorpal.build/source/rsync-{version}.tar.gz"
        source = ArtifactSource(name, path).build()

        step_script = (
            f'mkdir -p "$VORPAL_OUTPUT"\n'
            f'pushd ./source/{name}/{name}-{version}\n'
            f'./configure'
            f' --prefix="$VORPAL_OUTPUT"'
            f' --disable-openssl'
            f' --disable-xxhash'
            f' --disable-zstd'
            f' --disable-lz4\n'
            f'make\n'
            f'make install'
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
            .with_aliases([f"{name}:{version}"])
            .with_sources([source])
            .build(context)
        )
