"""OIDC auth-header tests + C6 secret-non-disclosure abuse cases.

Covers the blocking security controls for DKT-17:
  * 5-minute refresh buffer, refresh-token rotation, ``0o600`` rewrite.
  * No access/refresh token or Bearer header is ever logged or echoed into an
    error — verified by capturing stdout/stderr and inspecting exception text.
  * A malformed credentials file fails closed WITHOUT echoing its contents.

The OIDC HTTP calls are mocked (no live IdP). Runnable via ``pytest`` or
``python tests/test_auth.py``.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import stat
import sys
import time
import urllib.request
from pathlib import Path
from typing import Any

_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk import context as ctx  # noqa: E402

# Sentinel secret substrings that must NEVER appear in stdout/stderr/exceptions.
ACCESS_TOKEN = "ACCESS_TOKEN_SECRET_a1b2c3"
REFRESH_TOKEN = "REFRESH_TOKEN_SECRET_d4e5f6"
ROTATED_REFRESH = "ROTATED_REFRESH_SECRET_g7h8i9"
NEW_ACCESS = "NEW_ACCESS_TOKEN_SECRET_j0k1l2"

_TMP_COUNTER = [0]


def _tmp_path(name: str) -> str:
    base = os.environ.get("TMPDIR", "/tmp")
    _TMP_COUNTER[0] += 1
    return os.path.join(base, f"vorpal_test_{os.getpid()}_{_TMP_COUNTER[0]}_{name}")


def _write_creds(path: str, *, issued_at: int, expires_in: int, refresh: bool) -> None:
    creds: dict[str, Any] = {
        "registry": {"https://registry:50051": "https://issuer.example"},
        "issuer": {
            "https://issuer.example": {
                "access_token": ACCESS_TOKEN,
                "client_id": "cid",
                "expires_in": expires_in,
                "issued_at": issued_at,
                "refresh_token": REFRESH_TOKEN if refresh else "",
                "scopes": [],
            }
        },
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(creds, f)


class _FakeResp:
    def __init__(self, payload: dict[str, Any]) -> None:
        self._data = json.dumps(payload).encode("utf-8")

    def read(self) -> bytes:
        return self._data

    def __enter__(self) -> _FakeResp:
        return self

    def __exit__(self, *_: Any) -> bool:
        return False


@contextlib.contextmanager
def _mock_oidc(rotate: bool) -> Any:
    """Patch urlopen: discovery (str URL) -> token_endpoint; POST (Request) ->
    a fresh access token, optionally rotating the refresh token."""
    original = urllib.request.urlopen

    def fake(arg: Any, *_a: Any, **_k: Any) -> _FakeResp:
        if isinstance(arg, str):
            return _FakeResp({"token_endpoint": "https://issuer.example/token"})
        payload: dict[str, Any] = {"access_token": NEW_ACCESS, "expires_in": 3600}
        if rotate:
            payload["refresh_token"] = ROTATED_REFRESH
        return _FakeResp(payload)

    urllib.request.urlopen = fake  # type: ignore[assignment]
    try:
        yield
    finally:
        urllib.request.urlopen = original  # type: ignore[assignment]


def _assert_no_secrets(text: str) -> None:
    for secret in (ACCESS_TOKEN, REFRESH_TOKEN, ROTATED_REFRESH, NEW_ACCESS, "Bearer "):
        assert secret not in text, f"secret leaked: {secret!r} in {text!r}"


# ---------------------------------------------------------------------------
# Happy-path + buffer/rotation behavior
# ---------------------------------------------------------------------------


def test_missing_file_returns_none() -> None:
    assert ctx.client_auth_header("r", _tmp_path("missing")) is None


def test_no_registry_mapping_returns_none() -> None:
    path = _tmp_path("creds.json")
    _write_creds(path, issued_at=int(time.time()), expires_in=10000, refresh=True)
    try:
        assert ctx.client_auth_header("unmapped-registry", path) is None
    finally:
        os.unlink(path)


def test_valid_token_no_refresh_returns_bearer() -> None:
    path = _tmp_path("creds.json")
    _write_creds(path, issued_at=int(time.time()), expires_in=10000, refresh=True)
    try:
        header = ctx.client_auth_header("https://registry:50051", path)
        assert header == f"Bearer {ACCESS_TOKEN}"
        # File untouched (no refresh) — original access token intact.
        with open(path) as f:
            assert json.load(f)["issuer"]["https://issuer.example"][
                "access_token"
            ] == ACCESS_TOKEN
    finally:
        os.unlink(path)


def test_buffer_triggers_refresh_and_rotates_0o600() -> None:
    path = _tmp_path("creds.json")
    # Age 0 but expires_in=200 → age+300 >= 200 → refresh within the 5-min buffer.
    _write_creds(path, issued_at=int(time.time()), expires_in=200, refresh=True)
    try:
        with _mock_oidc(rotate=True):
            header = ctx.client_auth_header("https://registry:50051", path)
        assert header == f"Bearer {NEW_ACCESS}"
        with open(path) as f:
            iss = json.load(f)["issuer"]["https://issuer.example"]
        assert iss["access_token"] == NEW_ACCESS
        assert iss["refresh_token"] == ROTATED_REFRESH  # rotation persisted
        mode = stat.S_IMODE(os.stat(path).st_mode)
        assert mode == 0o600, f"expected 0o600, got {oct(mode)}"
    finally:
        os.unlink(path)


def test_refresh_without_rotation_keeps_old_refresh_token() -> None:
    path = _tmp_path("creds.json")
    _write_creds(path, issued_at=int(time.time()), expires_in=200, refresh=True)
    try:
        with _mock_oidc(rotate=False):
            ctx.client_auth_header("https://registry:50051", path)
        with open(path) as f:
            iss = json.load(f)["issuer"]["https://issuer.example"]
        assert iss["access_token"] == NEW_ACCESS
        assert iss["refresh_token"] == REFRESH_TOKEN  # unchanged when not rotated
    finally:
        os.unlink(path)


def test_outside_buffer_does_not_refresh() -> None:
    path = _tmp_path("creds.json")
    _write_creds(path, issued_at=int(time.time()), expires_in=10000, refresh=True)
    try:
        # urlopen left unpatched: if refresh were attempted it would hit the
        # network and fail; returning the original token proves no refresh.
        header = ctx.client_auth_header("https://registry:50051", path)
        assert header == f"Bearer {ACCESS_TOKEN}"
    finally:
        os.unlink(path)


def test_expired_without_refresh_token_raises_named_issuer() -> None:
    path = _tmp_path("creds.json")
    _write_creds(path, issued_at=int(time.time()), expires_in=200, refresh=False)
    try:
        ctx.client_auth_header("https://registry:50051", path)
    except RuntimeError as e:
        assert "issuer.example" in str(e)
        assert "vorpal login" in str(e)
        _assert_no_secrets(str(e))  # no token value in the remedy message
    else:
        raise AssertionError("expected RuntimeError")
    finally:
        os.unlink(path)


# ---------------------------------------------------------------------------
# C6 abuse cases — secret non-disclosure + fail-closed
# ---------------------------------------------------------------------------


def test_c6_no_secret_in_stdout_stderr_during_refresh() -> None:
    path = _tmp_path("creds.json")
    _write_creds(path, issued_at=int(time.time()), expires_in=200, refresh=True)
    out, err = io.StringIO(), io.StringIO()
    try:
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            with _mock_oidc(rotate=True):
                ctx.client_auth_header("https://registry:50051", path)
        _assert_no_secrets(out.getvalue())
        _assert_no_secrets(err.getvalue())
    finally:
        os.unlink(path)


def test_c6_malformed_credentials_fail_closed_without_echo() -> None:
    path = _tmp_path("creds.json")
    # Truncated JSON that embeds a secret-looking token fragment.
    with open(path, "w", encoding="utf-8") as f:
        f.write('{"issuer": {"x": {"access_token": "' + ACCESS_TOKEN + '"')
    try:
        ctx.client_auth_header("https://registry:50051", path)
    except RuntimeError as e:
        # Fails closed AND does not echo file contents / the token fragment.
        assert "invalid JSON" in str(e)
        assert ACCESS_TOKEN not in str(e)
    else:
        raise AssertionError("expected RuntimeError on malformed credentials")
    finally:
        os.unlink(path)


def test_c6_refresh_http_error_surfaces_status_not_body() -> None:
    import urllib.error

    path = _tmp_path("creds.json")
    _write_creds(path, issued_at=int(time.time()), expires_in=200, refresh=True)
    original = urllib.request.urlopen

    def fake(arg: Any, *_a: Any, **_k: Any) -> _FakeResp:
        if isinstance(arg, str):
            return _FakeResp({"token_endpoint": "https://issuer.example/token"})
        raise urllib.error.HTTPError(
            "https://issuer.example/token",
            400,
            f"error body with {REFRESH_TOKEN}",
            {},  # type: ignore[arg-type]
            None,
        )

    urllib.request.urlopen = fake  # type: ignore[assignment]
    try:
        ctx.client_auth_header("https://registry:50051", path)
    except RuntimeError as e:
        assert "status: 400" in str(e)
        _assert_no_secrets(str(e))  # response body (with token) not surfaced
    else:
        raise AssertionError("expected RuntimeError")
    finally:
        urllib.request.urlopen = original  # type: ignore[assignment]
        os.unlink(path)


def test_non_https_issuer_fails_closed() -> None:
    """http:// and file:// issuers must be rejected without echoing the token."""
    for bad_issuer in ("http://evil.example", "file:///etc/shadow"):
        path = _tmp_path("creds.json")
        # Expired token in the refresh buffer so the refresh path is taken.
        creds: Any = {
            "registry": {"https://registry:50051": bad_issuer},
            "issuer": {
                bad_issuer: {
                    "access_token": ACCESS_TOKEN,
                    "client_id": "cid",
                    "expires_in": 200,
                    "issued_at": int(time.time()),
                    "refresh_token": REFRESH_TOKEN,
                    "scopes": [],
                }
            },
        }
        with open(path, "w", encoding="utf-8") as f:
            json.dump(creds, f)
        try:
            ctx.client_auth_header("https://registry:50051", path)
        except RuntimeError as e:
            msg = str(e)
            assert "https" in msg.lower(), (
                f"issuer={bad_issuer!r}: expected https mention: {msg!r}"
            )
            _assert_no_secrets(msg)
        else:
            raise AssertionError(
                f"expected RuntimeError for non-https issuer {bad_issuer!r}"
            )
        finally:
            with contextlib.suppress(OSError):
                os.unlink(path)


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
