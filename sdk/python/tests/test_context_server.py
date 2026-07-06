"""ContextService server lifecycle — readiness line, GetArtifact(s) handlers,
and clean SIGTERM shutdown with port release.

Mirrors the TS SIGINT/SIGTERM handler (``context.ts:1053-1067``): the server
prints ``context service: [::]:PORT`` for CLI detection and exits cleanly on
SIGTERM, releasing the port so an immediate re-bind succeeds.

Runnable via ``pytest`` or ``python tests/test_context_server.py``.
"""

from __future__ import annotations

import select
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

import grpc

_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk import context as ctx  # noqa: E402
from vorpal_sdk.api.artifact import artifact_pb2  # noqa: E402
from vorpal_sdk.api.context import context_pb2_grpc  # noqa: E402


def _free_port() -> int:
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


_RUNNER = """
import sys
sys.path.insert(0, {src!r})
from vorpal_sdk.context import ConfigContext, _ConfigContextStore
from vorpal_sdk.api.artifact import artifact_pb2

store = _ConfigContextStore()
art = artifact_pb2.Artifact(name="srv", target=1)
art.systems.append(1)
store.artifact["deadbeef"] = art

c = ConfigContext("a", "/c", "ns", 1, False, None, None, {port}, "reg", store)
c.run()
"""


def test_handlers_return_artifacts_directly() -> None:
    """The servicer serves registered artifacts and sorts digests (no I/O)."""
    store = ctx._ConfigContextStore()
    art = artifact_pb2.Artifact(name="x", target=1)
    store.artifact["bbb"] = art
    store.artifact["aaa"] = art
    servicer = ctx._ContextServicer(store)
    resp = servicer.GetArtifacts(artifact_pb2.ArtifactsRequest(), None)  # type: ignore[arg-type]
    assert list(resp.digests) == ["aaa", "bbb"]


def test_sigterm_clean_exit_and_port_released() -> None:
    port = _free_port()
    src = str(_SRC)
    proc = subprocess.Popen(
        [sys.executable, "-c", _RUNNER.format(src=src, port=port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    # Wait for the readiness line on stdout (deadline-bounded).
    deadline = time.time() + 15
    ready = False
    assert proc.stdout is not None
    while time.time() < deadline:
        rlist, _, _ = select.select([proc.stdout], [], [], deadline - time.time())
        if not rlist:
            break
        line = proc.stdout.readline()
        if line == "":
            break  # EOF: process died before readiness
        if line.startswith("context service: "):
            assert f":{port}" in line
            ready = True
            break

    if not ready:
        proc.kill()
        err = proc.stderr.read() if proc.stderr else ""
        raise AssertionError(f"server never became ready. stderr:\n{err}")

    # Optional: prove the server actually accepts a call before shutdown.
    with grpc.insecure_channel(f"127.0.0.1:{port}") as channel:
        stub = context_pb2_grpc.ContextServiceStub(channel)
        resp = stub.GetArtifacts(artifact_pb2.ArtifactsRequest(), timeout=5)
        assert list(resp.digests) == ["deadbeef"]

    proc.send_signal(signal.SIGTERM)
    try:
        rc = proc.wait(timeout=15)
    except subprocess.TimeoutExpired:
        proc.kill()
        raise AssertionError("server did not exit within 15s of SIGTERM")

    assert rc == 0, f"expected clean exit 0, got {rc}"

    # Port released → an immediate re-bind to the same port succeeds.
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        s.bind(("127.0.0.1", port))
    finally:
        s.close()


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
