"""Cross-SDK digest-parity gate for the Python serializer.

The Python serializer (``vorpal_sdk.context``) MUST produce SHA-256 artifact
digests byte-identical to the Rust/Go/TS SDKs. This is the load-bearing
correctness property of the SDK family.

Golden digests in ``fixtures/digest-parity/digests.json`` are produced by the
canonical TypeScript reference serializer over the shared
``fixtures/digest-parity/artifacts.json``. To regenerate after a deliberate,
cross-SDK-coordinated format change, run the TS reference serializer
(``serializeArtifact``/``computeArtifactDigest`` in
``sdk/typescript/src/context.ts``) over ``artifacts.json`` and update
``digests.json`` — never edit goldens to match a Python change in isolation,
that would silently break parity with the other SDKs.

Runnable two ways:
  * ``pytest`` (collects the ``test_*`` functions), or
  * ``python tests/test_parity.py`` (dependency-free runner; exits non-zero on
    any failure) — used by the parity gate where pytest may be unavailable.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

# src-layout: make ``vorpal_sdk`` importable when the package is not installed
# (the parity gate runs against the source tree directly).
_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk.api.artifact import artifact_pb2  # noqa: E402
from vorpal_sdk.context import (  # noqa: E402
    artifact_to_json_bytes,
    compute_artifact_digest,
    serialize_artifact,
    serialize_artifact_source,
    serialize_artifact_step,
)

_FIXTURES = Path(__file__).resolve().parent / "fixtures" / "digest-parity"


def _load_fixtures() -> list[dict[str, Any]]:
    return json.loads((_FIXTURES / "artifacts.json").read_text(encoding="utf-8"))


def _load_goldens() -> dict[str, str]:
    data = json.loads((_FIXTURES / "digests.json").read_text(encoding="utf-8"))
    return data["digests"]


def build_artifact(d: dict[str, Any]) -> artifact_pb2.Artifact:
    """Build a proto Artifact from the neutral fixture dict.

    Presence is key-driven: an optional field (``digest``/``entrypoint``/
    ``script``) present in the dict is SET on the message (so ``HasField`` is
    true, even for ``""``); an absent key is left unset (``HasField`` false).
    This mirrors the TS golden generator's ``undefined``-vs-``""`` handling.
    """
    art = artifact_pb2.Artifact()
    art.target = d.get("target", 0)
    for s in d.get("sources", []):
        src = art.sources.add()
        if "digest" in s:
            src.digest = s["digest"]
        src.excludes.extend(s.get("excludes", []))
        src.includes.extend(s.get("includes", []))
        src.name = s.get("name", "")
        src.path = s.get("path", "")
    for st in d.get("steps", []):
        step = art.steps.add()
        if "entrypoint" in st:
            step.entrypoint = st["entrypoint"]
        if "script" in st:
            step.script = st["script"]
        for sec in st.get("secrets", []):
            secp = step.secrets.add()
            secp.name = sec["name"]
            secp.value = sec["value"]
        step.arguments.extend(st.get("arguments", []))
        step.artifacts.extend(st.get("artifacts", []))
        step.environments.extend(st.get("environments", []))
    art.systems.extend(d.get("systems", []))
    art.aliases.extend(d.get("aliases", []))
    art.name = d.get("name", "")
    return art


def test_digest_parity_all_fixtures() -> None:
    """Every fixture's Python digest equals the TS/Go/Rust golden digest."""
    goldens = _load_goldens()
    fixtures = _load_fixtures()
    assert {f["name"] for f in fixtures} == set(goldens), (
        "fixture set and golden set must cover the same names"
    )
    for f in fixtures:
        art = build_artifact(f["artifact"])
        got = compute_artifact_digest(art)
        want = goldens[f["name"]]
        # On mismatch, surface the produced canonical JSON to pinpoint the
        # diverging message/field (the 3am-diagnosability path).
        assert got == want, (
            f"digest mismatch for fixture {f['name']!r}: got {got}, want {want}\n"
            f"produced JSON: {artifact_to_json_bytes(art).decode('utf-8')}"
        )


def test_optional_present_empty_emits_empty_string() -> None:
    """HasField-not-truthiness: a present-but-empty optional emits "" not null."""
    src = artifact_pb2.ArtifactSource(digest="", name="n", path="p")
    assert src.HasField("digest") is True
    assert serialize_artifact_source(src)["digest"] == ""

    step = artifact_pb2.ArtifactStep(entrypoint="", script="")
    assert step.HasField("entrypoint") is True
    assert step.HasField("script") is True
    out = serialize_artifact_step(step)
    assert out["entrypoint"] == ""
    assert out["script"] == ""


def test_optional_absent_emits_null() -> None:
    """An absent optional emits null (None), distinct from present-empty."""
    src = artifact_pb2.ArtifactSource(name="n", path="p")
    assert src.HasField("digest") is False
    assert serialize_artifact_source(src)["digest"] is None

    step = artifact_pb2.ArtifactStep()
    assert step.HasField("entrypoint") is False
    assert step.HasField("script") is False
    out = serialize_artifact_step(step)
    assert out["entrypoint"] is None
    assert out["script"] is None


def test_empty_repeated_emits_array_not_null() -> None:
    """Empty repeated fields serialize as [] (never null), matching serde."""
    art = artifact_pb2.Artifact(name="x", target=1)
    obj = serialize_artifact(art)
    assert obj["sources"] == []
    assert obj["steps"] == []
    assert obj["systems"] == []
    assert obj["aliases"] == []
    step = artifact_pb2.ArtifactStep()
    sobj = serialize_artifact_step(step)
    for field in ("secrets", "arguments", "artifacts", "environments"):
        assert sobj[field] == []


def test_enums_serialize_as_integers() -> None:
    """Enum fields serialize as their integer value, not name strings."""
    art = artifact_pb2.Artifact(target=4, systems=[1, 4], name="e")
    obj = serialize_artifact(art)
    assert obj["target"] == 4
    assert obj["systems"] == [1, 4]
    assert isinstance(obj["target"], int)


def test_unknown_enum_int_not_coerced() -> None:
    """A1 abuse: an out-of-range enum integer is preserved, not coerced to a
    valid enum (which would silently change the digest)."""
    art = artifact_pb2.Artifact(name="u")
    art.target = 999  # proto3 open enum accepts unknown values
    obj = serialize_artifact(art)
    assert obj["target"] == 999


def test_json_metachars_do_not_break_structure() -> None:
    """A1 abuse: JSON metacharacters in a value stay inside their string and do
    not alter sibling fields — the parsed-back object round-trips exactly."""
    art = artifact_pb2.Artifact(name='a"b,c}d{e', target=1)
    step = art.steps.add()
    step.script = 'x"; }, {"injected": true}'
    parsed = json.loads(artifact_to_json_bytes(art).decode("utf-8"))
    assert parsed["name"] == 'a"b,c}d{e'
    assert parsed["steps"][0]["script"] == 'x"; }, {"injected": true}'
    # Sibling fields are unaffected by the metacharacters above.
    assert parsed["target"] == 1
    assert parsed["steps"][0]["entrypoint"] is None


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
        except Exception as exc:  # noqa: BLE001 - test runner surfaces any failure
            failures += 1
            print(f"FAIL {t.__name__}: {exc}")
    print(f"\n{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
