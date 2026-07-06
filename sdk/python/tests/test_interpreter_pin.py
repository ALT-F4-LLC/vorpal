"""Interpreter-pin conformance gate (drift-guard Part 2, ADR 0001 §7).

``test_version_equality.py`` enforces Part 1: the Rust/Go/TS
``DEFAULT_PYTHON_VERSION`` constants agree (with the Rust const canonical per
ADR 0001 Part B). This test enforces Part 2: the *sdk/python* interpreter pins
CONFORM to that canonical constant — closing the seam between the build-target
constant and the SDK's own pins, which Part 1 does not cover.

Two assertions:
  * ``sdk/python/.python-version`` byte-equals the canonical Rust
    ``DEFAULT_PYTHON_VERSION`` (extracted from ``sdk/rust/src/artifact/cpython.rs``
    by source text, same approach as test_version_equality.py — the Go const is
    unexported and not importable, so a grep/extract is the only cross-language
    comparison available).
  * ``requires-python`` in ``sdk/python/pyproject.toml`` is exactly
    ``">=3.13,<3.14"`` (the floor tracks the pinned 3.13.x; the ceiling excludes
    the next minor so a 3.14 interpreter cannot silently satisfy the project).

Fails CLOSED: a renamed/moved Rust const, a missing ``.python-version``, or a
missing ``requires-python`` makes the relevant read return nothing and FAILS —
a dropped pin must never pass by omission (same discipline as _extract).

Runnable two ways, matching test_version_equality.py:
  * ``pytest`` (collects the ``test_*`` functions), or
  * ``python tests/test_interpreter_pin.py`` (dependency-free runner; exits
    non-zero on any failure) — used by the parity gate where pytest may be
    unavailable.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# tests -> python -> sdk -> repo root
_REPO = Path(__file__).resolve().parents[3]

_CPYTHON_RS = _REPO / "sdk/rust/src/artifact/cpython.rs"
_PYTHON_VERSION = _REPO / "sdk/python/.python-version"
_PYPROJECT = _REPO / "sdk/python/pyproject.toml"

_REQUIRES_PYTHON = ">=3.13,<3.14"


def _canonical_python_version() -> str:
    """Extract the canonical Rust ``DEFAULT_PYTHON_VERSION`` literal.

    Mirrors ``test_version_equality.py._extract``; raises AssertionError if the
    declaration is absent (renamed/moved/reformatted) — a missing pin must fail.
    """
    text = _CPYTHON_RS.read_text(encoding="utf-8")
    match = re.search(
        r'^\s*(?:pub\s+)?const\s+DEFAULT_PYTHON_VERSION\s*(?::\s*&str\s*)?=\s*"([^"]+)"',
        text,
        re.MULTILINE,
    )
    assert match is not None, (
        f"could not extract const DEFAULT_PYTHON_VERSION from "
        f"{_CPYTHON_RS.relative_to(_REPO)} — renamed, moved, or reformatted? "
        f"(a missing version pin must not pass)"
    )
    return match.group(1)


def test_python_version_file_matches_canonical_constant() -> None:
    """sdk/python/.python-version == canonical Rust DEFAULT_PYTHON_VERSION."""
    assert _PYTHON_VERSION.is_file(), (
        f"{_PYTHON_VERSION.relative_to(_REPO)} is missing — the interpreter pin "
        f"must exist (a missing pin must not pass)"
    )
    pinned = _PYTHON_VERSION.read_text(encoding="utf-8").strip()
    canonical = _canonical_python_version()
    assert pinned == canonical, (
        f".python-version pins {pinned!r} but the canonical Rust "
        f"DEFAULT_PYTHON_VERSION is {canonical!r} — ADR 0001 §7 requires the "
        f"sdk/python interpreter pin to conform to the build-target constant"
    )


def test_requires_python_is_exact() -> None:
    """requires-python in pyproject.toml is exactly the agreed bound."""
    assert _PYPROJECT.is_file(), (
        f"{_PYPROJECT.relative_to(_REPO)} is missing"
    )
    # Source-text extraction (no tomllib) keeps this runnable on the same
    # interpreter range as test_version_equality.py's dependency-free runner.
    match = re.search(
        r'^\s*requires-python\s*=\s*"([^"]+)"',
        _PYPROJECT.read_text(encoding="utf-8"),
        re.MULTILINE,
    )
    assert match is not None, (
        f"requires-python is absent from {_PYPROJECT.relative_to(_REPO)} "
        f"— the interpreter bound must be declared (a missing bound must not pass)"
    )
    requires = match.group(1)
    assert requires == _REQUIRES_PYTHON, (
        f"requires-python is {requires!r} but must be exactly "
        f"{_REQUIRES_PYTHON!r} (floor tracks the pinned 3.13.x; ceiling excludes "
        f"the next minor)"
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
    if not failures:
        print(
            f"\n.python-version = {_canonical_python_version()}  "
            f"requires-python = {_REQUIRES_PYTHON}"
        )
    print(f"{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
