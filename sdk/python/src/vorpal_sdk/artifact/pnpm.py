"""pnpm package manager artifact (prebuilt binary).

Mirrors ``sdk/typescript/src/artifact/pnpm.ts``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext

DEFAULT_PNPM_VERSION = "10.30.3"


class Pnpm:
    """Builder for the pnpm package manager artifact."""

    def __init__(self) -> None:
        self._version = DEFAULT_PNPM_VERSION

    def with_version(self, version: str) -> Pnpm:
        self._version = version
        return self

    def build(self, context: ConfigContext) -> str:
        name = "pnpm"
        system = context.get_system()

        if system == artifact_pb2.AARCH64_DARWIN:
            source_target = "macos-arm64"
        elif system == artifact_pb2.AARCH64_LINUX:
            source_target = "linux-arm64"
        elif system == artifact_pb2.X8664_DARWIN:
            source_target = "macos-x64"
        elif system == artifact_pb2.X8664_LINUX:
            source_target = "linux-x64"
        else:
            raise ValueError(f"unsupported {name} system: {system}")

        source_version = self._version
        source_filename = f"pnpm-{source_version}-{source_target}"
        source_path = f"https://sdk.vorpal.build/source/{source_filename}"

        source = ArtifactSource(name, source_path).build()

        step_script = f"""mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/{name}/{source_filename}" "$VORPAL_OUTPUT/bin/pnpm"
chmod +x "$VORPAL_OUTPUT/bin/pnpm\""""
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
