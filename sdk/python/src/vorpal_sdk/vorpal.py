"""Config entry point — parses CLI args, dispatches on the artifact name, and
runs the ``ContextService``.

Mirrors the structure of ``sdk/typescript/src/vorpal.ts``: build the
:class:`~vorpal_sdk.context.ConfigContext`, register the requested artifact's
build graph, then serve the context until SIGINT/SIGTERM. New build cases slot
into ``main`` as their builders ship.
"""

from __future__ import annotations

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact.language.rust import Rust
from vorpal_sdk.context import ConfigContext

SYSTEMS = [
    artifact_pb2.AARCH64_DARWIN,
    artifact_pb2.AARCH64_LINUX,
    artifact_pb2.X8664_DARWIN,
    artifact_pb2.X8664_LINUX,
]


def build_vorpal(context: ConfigContext) -> str:
    return (
        Rust("vorpal", SYSTEMS)
        .with_bins(["vorpal"])
        .with_includes(["cli", "sdk/rust"])
        .with_packages(["vorpal-cli", "vorpal-sdk"])
        .build(context)
    )


def main() -> None:
    context = ConfigContext.create()

    if context.get_artifact_name() == "vorpal":
        build_vorpal(context)

    context.run()


if __name__ == "__main__":
    main()
