"""Builder-output parity gate (within-SDK-family, no CLI).

Each Python builder's produced ``Artifact`` digest MUST equal the Go SDK
builder output for the same inputs. This catches builder-level divergence
(environment ordering, default step-script text, secret sort) BEFORE the
CLI-gated cross-language e2e — distinct from the serializer digest-parity gate
(``test_parity.py``), which this builds on: the serializer is already proven
byte-identical to Go/TS, so a digest mismatch here pinpoints a BUILDER bug.

Goldens in ``fixtures/builder-parity/digests.json`` are produced by the Go SDK
builders; see ``fixtures/builder-parity/gen_goldens.go`` for the generator and
regen procedure. The inputs below MUST mirror that generator exactly — a
mismatch means either a real builder divergence or input drift (both fail
loudly, which is the point).

Targets the AARCH64_DARWIN system so ``shell`` dispatches to ``bash`` (pure);
the Linux ``bwrap`` path needs the linux-vorpal rootfs builder (a later phase).

Runnable two ways:
  * ``pytest`` (collects the ``test_*`` functions), or
  * ``python tests/test_builder_parity.py`` (dependency-free runner) — used by
    the parity gate where pytest may be unavailable.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk.api.artifact import artifact_pb2  # noqa: E402
from vorpal_sdk.artifact import (  # noqa: E402
    Artifact,
    ArtifactSource,
    DevelopmentEnvironment,
    Job,
    Process,
    UserEnvironment,
)
from vorpal_sdk.context import compute_artifact_digest  # noqa: E402
from vorpal_sdk.step import bash  # noqa: E402

_GOLDENS = (
    Path(__file__).resolve().parent
    / "fixtures"
    / "builder-parity"
    / "digests.json"
)


class _StubContext:
    """Minimal ``BuildContext`` that computes the pre-gRPC input digest.

    ``add_artifact`` returns ``compute_artifact_digest`` — exactly the digest
    the real ``ConfigContext`` computes from the un-hydrated artifact before
    the agent round-trip, which is what the Go/TS builders are compared on.
    """

    def get_system(self) -> artifact_pb2.ArtifactSystem:
        return artifact_pb2.AARCH64_DARWIN

    def get_variable(self, name: str) -> str | None:
        return None

    def get_artifact_namespace(self) -> str:
        return "altf4llc"

    def add_artifact(self, artifact: artifact_pb2.Artifact) -> str:
        return compute_artifact_digest(artifact)


def _load_goldens() -> dict[str, str]:
    data = json.loads(_GOLDENS.read_text(encoding="utf-8"))
    digests: dict[str, str] = data["digests"]
    return digests


def _build_all() -> dict[str, str]:
    ctx = _StubContext()
    darwin = artifact_pb2.AARCH64_DARWIN

    job = (
        Job("vorpal-job-test", "echo hello", [darwin])
        .with_artifacts(["digabc"])
        .with_secrets({"B_KEY": "b", "A_KEY": "a"})
        .build(ctx)
    )

    process = (
        Process("proc", "/bin/server", [darwin])
        .with_arguments(["--port", "8080"])
        .with_artifacts(["dig1", "dig2"])
        .with_secrets({"TOKEN": "x"})
        .build(ctx)
    )

    devenv = (
        DevelopmentEnvironment("dev", [darwin])
        .with_artifacts(["digtool"])
        .with_environments(["FOO=bar", "PATH=/custom/bin"])
        .with_secrets({"S": "v"})
        .build(ctx)
    )

    userenv = (
        UserEnvironment("user", [darwin])
        .with_artifacts(["diguser"])
        .with_symlinks([("/src/b", "/dst/b"), ("/src/a", "/dst/a")])
        .build(ctx)
    )

    # artifact-sources: dedup is exercised by the duplicate "s1" source and the
    # duplicate "x" alias, both of which must collapse to a single entry.
    step = bash(
        ["digart"],
        ["ENVKEY=enval"],
        [artifact_pb2.ArtifactStepSecret(name="KEY", value="val")],
        "echo s",
    )
    s1 = ArtifactSource("s1", "/p1").build()
    s1_dup = ArtifactSource("s1", "/other").build()
    s2 = (
        ArtifactSource("s2", "/p2")
        .with_digest("srcdig")
        .with_excludes(["*.log"])
        .with_includes(["src/**"])
        .build()
    )
    artifact_sources = (
        Artifact("multi", [step], [darwin])
        .with_sources([s1, s2, s1_dup])
        .with_aliases(["x", "x", "y"])
        .build(ctx)
    )

    return {
        "job": job,
        "process": process,
        "devenv": devenv,
        "userenv": userenv,
        "artifact-sources": artifact_sources,
    }


def test_builder_output_parity_all_fixtures() -> None:
    """Every Python builder digest equals the Go builder golden."""
    goldens = _load_goldens()
    produced = _build_all()
    assert set(produced) == set(goldens), (
        "produced builder set and golden set must cover the same names"
    )
    for name, got in produced.items():
        want = goldens[name]
        assert got == want, (
            f"builder digest mismatch for {name!r}: got {got}, want {want}"
        )


def _run() -> int:
    tests = [
        v
        for k, v in sorted(globals().items())
        if k.startswith("test_") and callable(v)
    ]
    failures = 0
    for t in tests:
        try:
            t()
            print(f"PASS {t.__name__}")
        except Exception as exc:  # noqa: BLE001 - runner surfaces any failure
            failures += 1
            print(f"FAIL {t.__name__}: {exc}")
    print(f"\n{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
