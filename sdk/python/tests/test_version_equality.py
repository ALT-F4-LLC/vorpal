"""Cross-SDK version-equality gate for the build-target Python toolchain.

DEFAULT_PYTHON_VERSION and DEFAULT_UV_VERSION are pinned independently in each
SDK (no shared config layer between Rust/Go/TS). ADR 0001 Part B makes the Rust
``DEFAULT_PYTHON_VERSION`` the canonical source of truth and the Go/TS constants
conforming copies; this test enforces the no-drift invariant by comparing the
SOURCE TEXT of all three constants and asserting exactly ONE distinct value.

The comparison is by source text on purpose (DKT-19 design note, DKT-4/5
review): the Go consts are UNEXPORTED (``const defaultPythonVersion``, per-SDK
convention mirroring ``bun.go``), so they are NOT cross-package importable — a
programmatic import comparison is impossible. Grep/extract the literal instead.

A constant that has been renamed or moved out of its file makes extraction
return nothing, which FAILS the test (a missing pin must never pass silently).

Runnable two ways, matching ``test_parity.py``:
  * ``pytest`` (collects the ``test_*`` functions), or
  * ``python tests/test_version_equality.py`` (dependency-free runner; exits
    non-zero on any failure) — used by the parity gate where pytest may be
    unavailable.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# tests -> python -> sdk -> repo root
_REPO = Path(__file__).resolve().parents[3]

# (constant, [(sdk, source-file, identifier-as-written)]). Identifiers differ in
# case because Go keeps the const unexported (lowerCamel) while Rust/TS export an
# UPPER_SNAKE name — the extraction is name-agnostic, see _extract.
_CONSTANTS: dict[str, list[tuple[str, Path, str]]] = {
    "DEFAULT_PYTHON_VERSION": [
        ("rust", _REPO / "sdk/rust/src/artifact/cpython.rs", "DEFAULT_PYTHON_VERSION"),
        ("go", _REPO / "sdk/go/pkg/artifact/cpython.go", "defaultPythonVersion"),
        ("typescript", _REPO / "sdk/typescript/src/artifact/cpython.ts", "DEFAULT_PYTHON_VERSION"),
    ],
    "DEFAULT_UV_VERSION": [
        ("rust", _REPO / "sdk/rust/src/artifact/uv.rs", "DEFAULT_UV_VERSION"),
        ("go", _REPO / "sdk/go/pkg/artifact/uv.go", "defaultUvVersion"),
        ("typescript", _REPO / "sdk/typescript/src/artifact/uv.ts", "DEFAULT_UV_VERSION"),
    ],
}


def _extract(path: Path, identifier: str) -> str:
    """Return the string literal assigned to ``identifier`` in ``path``.

    Matches the three declaration forms across SDKs in one pattern:
      Rust  ``pub const DEFAULT_PYTHON_VERSION: &str = "3.13.14";``
      Go    ``const defaultPythonVersion = "3.13.14"``
      TS    ``export const DEFAULT_PYTHON_VERSION = "3.13.14";``
    Raises AssertionError if the declaration is absent (renamed/moved) — a
    missing pin must fail, never pass by omission.
    """
    text = path.read_text(encoding="utf-8")
    pattern = re.compile(
        r"^\s*(?:pub\s+|export\s+)?const\s+"
        + re.escape(identifier)
        + r"\s*(?::\s*&str\s*)?=\s*\"([^\"]+)\"",
        re.MULTILINE,
    )
    match = pattern.search(text)
    assert match is not None, (
        f"could not extract const {identifier!r} from {path.relative_to(_REPO)} "
        f"— renamed, moved, or reformatted? (a missing version pin must not pass)"
    )
    return match.group(1)


def _assert_single_value(constant: str) -> str:
    extracted = {
        sdk: _extract(path, identifier)
        for sdk, path, identifier in _CONSTANTS[constant]
    }
    distinct = set(extracted.values())
    assert len(distinct) == 1, (
        f"{constant} diverges across SDKs: {extracted} — ADR 0001 Part B requires "
        f"the Go/TS copies to conform to the canonical Rust constant in lockstep"
    )
    return distinct.pop()


def test_default_python_version_equal_across_sdks() -> None:
    """All three SDKs pin the same DEFAULT_PYTHON_VERSION source-text value."""
    _assert_single_value("DEFAULT_PYTHON_VERSION")


def test_default_uv_version_equal_across_sdks() -> None:
    """All three SDKs pin the same DEFAULT_UV_VERSION source-text value."""
    _assert_single_value("DEFAULT_UV_VERSION")


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
    # Echo the agreed values for traceability when run as a script.
    if not failures:
        py = _assert_single_value("DEFAULT_PYTHON_VERSION")
        uv = _assert_single_value("DEFAULT_UV_VERSION")
        print(f"\nDEFAULT_PYTHON_VERSION = {py}  DEFAULT_UV_VERSION = {uv}")
    print(f"{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
