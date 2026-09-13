"""Node.js runtime artifact (official binary distribution).

Mirrors ``sdk/typescript/src/artifact/nodejs.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell
from vorpal_sdk.system import SYSTEMS, get_system_str

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class NodeJS:
    """Builder for the Node.js runtime artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "nodejs"
        system = context.get_system()
        system_str = get_system_str(system)

        if system_str == "aarch64-darwin":
            source_target = "darwin-arm64"
        elif system_str == "aarch64-linux":
            source_target = "linux-arm64"
        elif system_str == "x86_64-darwin":
            source_target = "darwin-x64"
        elif system_str == "x86_64-linux":
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

        return (
            Artifact(name, steps, SYSTEMS)
            .with_aliases([f"{name}:{source_version}"])
            .with_sources([source])
            .build(context)
        )
