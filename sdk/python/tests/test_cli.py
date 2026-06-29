"""CLI argument parsing tests — behavioral parity with ``cli.ts``.

Runnable two ways:
  * ``pytest`` (collects the ``test_*`` functions), or
  * ``python tests/test_cli.py`` (dependency-free runner; exits non-zero on
    any failure).
"""

from __future__ import annotations

import sys
from pathlib import Path

_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk.cli import parse_cli_args  # noqa: E402

_REQUIRED = [
    "start",
    "--artifact", "vorpal",
    "--artifact-context", "/ctx",
    "--artifact-namespace", "library",
    "--artifact-system", "aarch64-darwin",
    "--port", "50051",
]


def test_parses_full_flag_set() -> None:
    cmd = parse_cli_args(
        [
            "start",
            "--agent", "https://agent:50051",
            "--artifact", "vorpal",
            "--artifact-context", "/ctx",
            "--artifact-namespace", "library",
            "--artifact-system", "aarch64-darwin",
            "--port", "50051",
            "--registry", "https://registry:50051",
            "--artifact-unlock",
            "--artifact-variable", "FOO=bar",
            "--artifact-variable", "BAZ=qux",
        ]
    )
    assert cmd.agent == "https://agent:50051"
    assert cmd.artifact == "vorpal"
    assert cmd.artifact_context == "/ctx"
    assert cmd.artifact_namespace == "library"
    assert cmd.artifact_system == "aarch64-darwin"
    assert cmd.port == 50051
    assert cmd.registry == "https://registry:50051"
    assert cmd.artifact_unlock is True
    assert cmd.artifact_variable == ["FOO=bar", "BAZ=qux"]


def test_artifact_variable_repeat_semantics() -> None:
    cmd = parse_cli_args(
        _REQUIRED + ["--artifact-variable", "A=1", "--artifact-variable", "B=2"]
    )
    assert cmd.artifact_variable == ["A=1", "B=2"]


def test_defaults_unlock_false_and_socket_address() -> None:
    cmd = parse_cli_args(_REQUIRED)
    assert cmd.artifact_unlock is False
    assert cmd.agent.startswith("unix://")
    assert cmd.registry.startswith("unix://")
    assert cmd.artifact_variable == []


def test_missing_start_subcommand_raises() -> None:
    try:
        parse_cli_args(["--artifact", "x"])
    except ValueError as e:
        assert "start" in str(e)
    else:
        raise AssertionError("expected ValueError")


def test_required_flags_enforced() -> None:
    for missing in (
        "--artifact",
        "--artifact-context",
        "--artifact-namespace",
        "--artifact-system",
        "--port",
    ):
        args = ["start"]
        i = 1
        skip_next = False
        for tok in _REQUIRED[1:]:
            if skip_next:
                skip_next = False
                continue
            if tok == missing:
                skip_next = True  # drop the flag's value too
                continue
            args.append(tok)
            i += 1
        try:
            parse_cli_args(args)
        except ValueError as e:
            assert "required" in str(e)
        else:
            raise AssertionError(f"expected ValueError for missing {missing}")


def test_unknown_argument_raises() -> None:
    try:
        parse_cli_args(_REQUIRED + ["--nope"])
    except ValueError as e:
        assert "unknown argument" in str(e)
    else:
        raise AssertionError("expected ValueError")


def test_flag_missing_value_raises() -> None:
    try:
        parse_cli_args(["start", "--artifact"])
    except ValueError as e:
        assert "requires a value" in str(e)
    else:
        raise AssertionError("expected ValueError")


def test_invalid_port_raises() -> None:
    args = [
        "start",
        "--artifact", "v",
        "--artifact-context", "/c",
        "--artifact-namespace", "n",
        "--artifact-system", "aarch64-darwin",
        "--port", "notaport",
    ]
    try:
        parse_cli_args(args)
    except ValueError as e:
        assert "not a valid number" in str(e)
    else:
        raise AssertionError("expected ValueError")


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
