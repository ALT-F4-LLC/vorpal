"""TLS-downgrade abuse case + URI->target conversion — parity with
``context.ts:38-73``.

The blocking invariant: ``get_client_credentials`` returns ``None`` (insecure)
ONLY for the local ``http://``/``unix://`` schemes. For every non-local scheme
it returns TLS credentials and NEVER ``None`` — even when the CA file is absent
(system-root TLS), there is no insecure downgrade.

Runnable via ``pytest`` or ``python tests/test_tls.py``.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

import grpc

_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk import context as ctx  # noqa: E402

_TMP_COUNTER = [0]


def _tmp_path(name: str) -> str:
    base = os.environ.get("TMPDIR", "/tmp")
    _TMP_COUNTER[0] += 1
    return os.path.join(base, f"vorpal_tls_{os.getpid()}_{_TMP_COUNTER[0]}_{name}")


def test_local_schemes_are_insecure() -> None:
    assert ctx.get_client_credentials("http://localhost:50051") is None
    assert ctx.get_client_credentials("unix:///var/lib/vorpal/vorpal.sock") is None


def test_non_local_scheme_never_insecure_when_ca_absent() -> None:
    original = ctx.VORPAL_CA_PATH
    ctx.VORPAL_CA_PATH = _tmp_path("definitely-absent-ca.pem")
    try:
        assert not os.path.exists(ctx.VORPAL_CA_PATH)
        creds = ctx.get_client_credentials("https://registry:50051")
        # CA absent → system-root TLS, NOT None (insecure) and NOT a refusal.
        assert creds is not None
        assert isinstance(creds, grpc.ChannelCredentials)
    finally:
        ctx.VORPAL_CA_PATH = original


def test_non_local_scheme_uses_pinned_ca_when_present() -> None:
    ca_path = _tmp_path("ca.pem")
    with open(ca_path, "wb") as f:
        f.write(b"-----BEGIN CERTIFICATE-----\nDEADBEEF\n-----END CERTIFICATE-----\n")
    original = ctx.VORPAL_CA_PATH
    ctx.VORPAL_CA_PATH = ca_path
    try:
        creds = ctx.get_client_credentials("https://registry:50051")
        assert creds is not None
        assert isinstance(creds, grpc.ChannelCredentials)
    finally:
        ctx.VORPAL_CA_PATH = original
        os.unlink(ca_path)


def test_to_grpc_target_conversions() -> None:
    assert ctx.to_grpc_target("unix:///p.sock") == "unix:///p.sock"
    assert ctx.to_grpc_target("https://host") == "host:443"
    assert ctx.to_grpc_target("https://host:1234") == "host:1234"
    assert ctx.to_grpc_target("https://host/") == "host:443"
    assert ctx.to_grpc_target("http://host") == "host:80"
    assert ctx.to_grpc_target("http://host:8080") == "host:8080"
    assert ctx.to_grpc_target("host:9999") == "host:9999"


def test_unix_target_pins_valid_authority() -> None:
    # grpcio would otherwise send a percent-encoded socket path as :authority,
    # which strict h2 servers reject. Pin a valid authority for an insecure
    # (credentials is None) unix target only.
    assert ctx._channel_options("unix:///tmp/vorpal.sock", None) == [
        ("grpc.default_authority", "localhost")
    ]
    assert ctx._channel_options("host:443", None) == []


def test_unix_scheme_is_insecure_and_pins_authority() -> None:
    # AC #2: the canonical unix scheme selects insecure (non-TLS) credentials
    # AND its target receives the localhost authority override.
    uri = "unix:///var/lib/vorpal/vorpal.sock"
    credentials = ctx.get_client_credentials(uri)
    target = ctx.to_grpc_target(uri)
    assert credentials is None  # insecure, not TLS
    assert target.startswith("unix:")
    assert ctx._channel_options(target, credentials) == [
        ("grpc.default_authority", "localhost")
    ]


def test_non_unix_target_gets_no_authority_override() -> None:
    # AC #3: a non-unix (TLS) target never receives the authority override.
    original = ctx.VORPAL_CA_PATH
    ctx.VORPAL_CA_PATH = _tmp_path("definitely-absent-ca.pem")
    try:
        uri = "https://registry:50051"
        credentials = ctx.get_client_credentials(uri)
        target = ctx.to_grpc_target(uri)
        assert credentials is not None  # TLS
        assert ctx._channel_options(target, credentials) == []
    finally:
        ctx.VORPAL_CA_PATH = original


def test_tls_channel_never_gets_authority_override_for_ambiguous_unix_target() -> None:
    # AC #1: the non-canonical `unix:relative/path` target matches the loose
    # `unix:` prefix but NOT the `unix://` scheme, so get_client_credentials
    # selects TLS. Gating the override on `credentials is None` guarantees a
    # TLS channel can never also receive the localhost authority override — the
    # insecure combination the DKT-28 reviewers flagged.
    original = ctx.VORPAL_CA_PATH
    ctx.VORPAL_CA_PATH = _tmp_path("definitely-absent-ca.pem")
    try:
        for uri in ("unix:relative/path", "unix:/absolute/path"):
            credentials = ctx.get_client_credentials(uri)
            target = ctx.to_grpc_target(uri)
            assert credentials is not None  # TLS creds selected for this form
            assert target.startswith("unix:")  # matches the loose unix: prefix
            # Despite the unix: prefix, no override because credentials are TLS.
            assert ctx._channel_options(target, credentials) == []
    finally:
        ctx.VORPAL_CA_PATH = original


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
