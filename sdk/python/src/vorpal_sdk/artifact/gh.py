"""GitHub CLI (gh) artifact (prebuilt release archive).

Mirrors ``sdk/typescript/src/artifact/gh.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell
from vorpal_sdk.system import SYSTEMS, get_system_str

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext

DEFAULT_GH_VERSION = "2.87.3"


class Gh:
    """Builder for the GitHub CLI artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "gh"
        system = context.get_system()
        system_str = get_system_str(system)

        if system_str == "aarch64-darwin":
            source_target = "macOS_arm64"
            source_extension = "zip"
        elif system_str == "aarch64-linux":
            source_target = "linux_arm64"
            source_extension = "tar.gz"
        elif system_str == "x86_64-darwin":
            source_target = "macOS_amd64"
            source_extension = "zip"
        elif system_str == "x86_64-linux":
            source_target = "linux_amd64"
            source_extension = "tar.gz"
        else:
            raise ValueError(f"unsupported {name} system: {system}")

        source_version = DEFAULT_GH_VERSION
        source_path = (
            f"https://sdk.vorpal.build/source/"
            f"gh_{source_version}_{source_target}.{source_extension}"
        )

        source = ArtifactSource(name, source_path).build()

        cp_line = (
            f'cp -pr "source/{name}/gh_{source_version}_{source_target}/bin/gh" '
            f'"$VORPAL_OUTPUT/bin/gh"'
        )
        step_script = (
            'mkdir -p "$VORPAL_OUTPUT/bin"\n\n'
            f"{cp_line}\n\n"
            'chmod +x "$VORPAL_OUTPUT/bin/gh"'
        )

        steps = [shell(context, [], [], step_script, [])]

        return (
            Artifact(name, steps, SYSTEMS)
            .with_aliases([f"{name}:{source_version}"])
            .with_sources([source])
            .build(context)
        )
