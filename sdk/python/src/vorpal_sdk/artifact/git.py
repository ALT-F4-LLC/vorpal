"""Git artifact (built from source via configure+make).

Mirrors ``sdk/typescript/src/artifact/git.js``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.artifact import Artifact, ArtifactSource
from vorpal_sdk.step import shell
from vorpal_sdk.system import SYSTEMS

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class Git:
    """Builder for the Git artifact."""

    def build(self, context: ConfigContext) -> str:
        name = "git"
        source_version = "2.53.0"
        source_path = (
            f"https://sdk.vorpal.build/source/git-{source_version}.tar.gz"
        )

        source = ArtifactSource(name, source_path).build()

        step_script = f"""mkdir -p "$VORPAL_OUTPUT/bin"

pushd ./source/{name}/git-{source_version}

./configure --prefix=$VORPAL_OUTPUT

make
make install"""

        steps = [shell(context, [], [], step_script, [])]

        return (
            Artifact(name, steps, SYSTEMS)
            .with_aliases([f"{name}:{source_version}"])
            .with_sources([source])
            .build(context)
        )
