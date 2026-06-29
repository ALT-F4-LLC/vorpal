"""Clippy artifact (prebuilt from static.rust-lang.org).

Mirrors ``sdk/typescript/src/artifact/clippy.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.artifact.rust_toolchain import (
    RUST_TOOLCHAIN_VERSION,
    rust_toolchain_target,
)
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Clippy:
    """Builder for the Clippy artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "clippy"
        source_target = rust_toolchain_target(context.get_system())
        source_version = RUST_TOOLCHAIN_VERSION
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"{name}-{source_version}-{source_target}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        step_script = (
            f'cp -pr "./source/{name}/{name}-{source_version}-'
            f'{source_target}/{name}-preview/." "$VORPAL_OUTPUT"'
        )
        steps = [shell(context, [], [], step_script, [])]
        systems = [
            artifact_pb2.AARCH64_DARWIN,
            artifact_pb2.AARCH64_LINUX,
            artifact_pb2.X8664_DARWIN,
            artifact_pb2.X8664_LINUX,
        ]

        return (
            Artifact(name, steps, systems).with_sources([source]).build(context)
        )
