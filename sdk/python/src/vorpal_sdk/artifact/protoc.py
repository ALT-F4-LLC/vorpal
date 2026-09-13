"""protoc (Protocol Buffers compiler) artifact.

Mirrors ``sdk/typescript/src/artifact/protoc.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell
from vorpal_sdk.system import SYSTEMS, get_system_str

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Protoc:
    """Builder for the protoc artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "protoc"
        system = context.get_system()
        system_str = get_system_str(system)

        if system_str == "aarch64-darwin":
            source_target = "osx-aarch_64"
        elif system_str == "aarch64-linux":
            source_target = "linux-aarch_64"
        elif system_str == "x86_64-darwin":
            source_target = "osx-x86_64"
        elif system_str == "x86_64-linux":
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

        return (
            Artifact(name, steps, SYSTEMS)
            .with_aliases([f"{name}:{source_version}"])
            .with_sources([source])
            .build(context)
        )
