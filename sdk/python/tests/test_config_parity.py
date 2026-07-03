"""Cross-LANGUAGE config-parity e2e (DKT-20 AC3, python-sdk.md Phase 7).

Building the SAME artifact through the Python root config (``Vorpal.py.toml``)
must yield the SAME artifact digest as building it through the Go/TS configs
(``Vorpal.go.toml`` / ``Vorpal.ts.toml``) — the cross-LANGUAGE authoring
invariant, distinct from the within-SDK serializer parity (``test_parity.py``)
and the within-SDK-family builder parity (``test_builder_parity.py``). The
invocation mirrors the sdk-parity skill and the CI workflow:
``vorpal build --config <Vorpal.X.toml> <artifact>`` prints the artifact digest
on stdout.

RUNTIME-GATED. A green run needs the full build env that DKT-20 does NOT own:
the ``vorpal`` CLI, running services, the published ``cpython``/``uv`` mirror
(DKT-25 — returns 403 today), and the provisioned pinned toolchain (DKT-21).
Until then a build fails closed at the C1 mint gate (``unpinned - use
--unlock``). So this test SKIPS — never hard-fails — whenever the CLI is absent
or a build does not succeed; it asserts digest equality ONLY when both the
Python and the reference build succeed, at which point a mismatch is a real
cross-language divergence. The build-success gate itself lives in the sdk-parity
skill / e2e harness; this test is the offline-runnable digest-equality scaffold
that goes green with no edit once DKT-21/DKT-25 land.

A skip is NOT a pass: the standalone runner prints SKIP distinctly and the
parity gate must treat a skip as "not yet verified", never as green.

Runnable two ways, matching test_supply_chain.py:
  * ``pytest``, or
  * ``python tests/test_config_parity.py`` (dependency-free; exits non-zero only
    on a real digest mismatch, never on a SKIP).
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

# tests -> python -> sdk -> repo root
_REPO = Path(__file__).resolve().parents[3]

_PYTHON_CONFIG = "Vorpal.py.toml"
# The reference config to compare against; Go is the canonical reference SDK
# (override with $VORPAL_PARITY_REFERENCE_CONFIG, e.g. Vorpal.ts.toml).
_REFERENCE_CONFIG = os.environ.get("VORPAL_PARITY_REFERENCE_CONFIG", "Vorpal.go.toml")
# Default artifact mirrors the sdk-parity skill default.
_ARTIFACT = os.environ.get("VORPAL_PARITY_ARTIFACT", "vorpal")


class _Skipped(Exception):
    """Raised by the standalone runner to mark a gated, not-yet-runnable test."""


def _skip(reason: str) -> None:
    """Skip under pytest if present, else raise the standalone sentinel."""
    pytest = sys.modules.get("pytest")
    if pytest is not None:
        pytest.skip(reason)
    raise _Skipped(reason)


def _vorpal_bin() -> str:
    vorpal = os.environ.get("VORPAL_BIN") or shutil.which("vorpal")
    if not vorpal:
        _skip(
            "vorpal CLI absent — cross-language build needs a build env "
            "(DKT-21). Set $VORPAL_BIN."
        )
    return vorpal  # type: ignore[return-value]


def _build_digest(vorpal: str, config: str) -> str:
    """Build ``_ARTIFACT`` via ``config`` and return the digest from stdout.

    A non-zero build is treated as GATED (skip), not a failure: pre-DKT-21/25
    the toolchain mirror is unpublished and the build fails closed at the mint
    gate, so a build that does not succeed means the env is not yet ready — the
    e2e is not meaningful, not broken.
    """
    result = subprocess.run(
        [vorpal, "build", "--config", config, _ARTIFACT],
        cwd=_REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        tail = (result.stderr or result.stdout).strip().splitlines()[-3:]
        _skip(
            f"`vorpal build --config {config} {_ARTIFACT}` did not succeed "
            f"(toolchain mirror/provisioning not ready — DKT-21/DKT-25):\n"
            + "\n".join(tail)
        )
    digest = result.stdout.strip().splitlines()
    if not digest:
        _skip(
            f"`vorpal build --config {config} {_ARTIFACT}` produced no digest on "
            f"stdout — build env not ready (DKT-21/DKT-25)."
        )
    return digest[-1].strip()


def test_python_config_digest_equals_reference() -> None:
    """Vorpal.py.toml and the reference config build the same artifact digest."""
    vorpal = _vorpal_bin()
    py_digest = _build_digest(vorpal, _PYTHON_CONFIG)
    ref_digest = _build_digest(vorpal, _REFERENCE_CONFIG)
    assert py_digest == ref_digest, (
        f"cross-language digest mismatch for artifact {_ARTIFACT!r}: "
        f"{_PYTHON_CONFIG}={py_digest} but {_REFERENCE_CONFIG}={ref_digest} — "
        f"the Python config does not author the same artifact as the reference"
    )


def _run() -> int:
    tests = [
        v
        for k, v in sorted(globals().items())
        if k.startswith("test_") and callable(v)
    ]
    failures = skipped = 0
    for t in tests:
        try:
            t()
            print(f"PASS {t.__name__}")
        except _Skipped as exc:
            skipped += 1
            print(f"SKIP {t.__name__}: {exc}")
        except Exception as exc:  # noqa: BLE001 - runner surfaces any real failure
            failures += 1
            print(f"FAIL {t.__name__}: {exc}")
    passed = len(tests) - failures - skipped
    print(f"\n{passed} passed, {skipped} skipped (gated), {failures} failed")
    if skipped and not failures and not passed:
        print(
            "NOTE: all tests gated — cross-language config parity NOT yet "
            "verified (awaiting DKT-21/DKT-25)."
        )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
