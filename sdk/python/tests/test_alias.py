"""Artifact alias parse/format tests — parity with ``context.ts:353-510``.

Runnable via ``pytest`` or ``python tests/test_alias.py``.
"""

from __future__ import annotations

import sys
from pathlib import Path

_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk.context import (  # noqa: E402
    ArtifactAlias,
    format_artifact_alias,
    parse_artifact_alias,
)


def test_bare_name_applies_defaults() -> None:
    a = parse_artifact_alias("my-tool")
    assert (a.name, a.namespace, a.tag) == ("my-tool", "library", "latest")


def test_name_with_tag() -> None:
    a = parse_artifact_alias("my-tool:v1.0")
    assert (a.name, a.namespace, a.tag) == ("my-tool", "library", "v1.0")


def test_namespace_name_tag() -> None:
    a = parse_artifact_alias("ns/my-tool:v2")
    assert (a.name, a.namespace, a.tag) == ("my-tool", "ns", "v2")


def test_format_omits_defaults() -> None:
    assert format_artifact_alias(ArtifactAlias("t", "library", "latest")) == "t"
    assert format_artifact_alias(ArtifactAlias("t", "ns", "latest")) == "ns/t"
    assert format_artifact_alias(ArtifactAlias("t", "library", "v1")) == "t:v1"
    assert format_artifact_alias(ArtifactAlias("t", "ns", "v1")) == "ns/t:v1"


def test_roundtrip_non_default() -> None:
    s = "linux-vorpal-slim:edge"
    assert format_artifact_alias(parse_artifact_alias("ns/" + s)) == "ns/" + s


def _expect_error(alias: str, fragment: str) -> None:
    try:
        parse_artifact_alias(alias)
    except ValueError as e:
        assert fragment in str(e), f"{alias!r}: {e}"
    else:
        raise AssertionError(f"expected ValueError for {alias!r}")


def test_empty_alias_raises() -> None:
    _expect_error("", "empty")


def test_too_long_raises() -> None:
    _expect_error("a" * 256, "too long")


def test_empty_tag_raises() -> None:
    _expect_error("name:", "tag cannot be empty")


def test_empty_namespace_raises() -> None:
    _expect_error("/name", "namespace cannot be empty")


def test_too_many_separators_raises() -> None:
    _expect_error("a/b/c", "too many path separators")


def test_invalid_name_chars_raises() -> None:
    _expect_error("na me", "name contains invalid")


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
        except Exception as exc:  # noqa: BLE001
            failures += 1
            print(f"FAIL {t.__name__}: {exc}")
    print(f"\n{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
