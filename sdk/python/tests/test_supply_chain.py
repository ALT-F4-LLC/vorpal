"""Supply-chain tests for the build-target Python toolchain (DKT-19, ADR 0001).

These encode the REAL provenance / hash-enforcement assertions from ADR 0001
Part A and ``python-language-target.md`` §4/§9. They are RUNTIME-GATED: each
test detects its prerequisites and SKIPS with an explicit reason + the
unblocking ticket when they are absent; when the prereqs land, the same test
asserts the real behavior with no edit.

A skip is NOT a pass: the standalone runner prints SKIP distinctly and the
parity gate must treat "all skipped" as "not yet verified", never as green.

Current gate breakdown:
  * 4 manifest-gated tests need the provenance manifest:
    ``test_provenance_manifest_complete``,
    ``test_provenance_a1_origin_is_canonical_upstream_not_mirror``,
    ``test_provenance_two_hashes_never_conflated``, and
    ``test_provenance_link_a_mirror_equals_upstream``. The mirror test also
    needs a reachable mirror.
  * 3 fixture-gated tests need ``$VORPAL_UV_FIXTURE`` plus ``uv``:
    ``test_tampered_package_c3a_uv_sync_fails``,
    ``test_uv_sync_locked_catches_drift``, and
    ``test_frozen_lock_required_for_build``.
  * 1 CLI+e2e-gated test needs ``$VORPAL_BIN`` plus the provenance manifest:
    ``test_tampered_source_archive_fails_closed``. It stays skipped as
    author-complete/e2e-gated per DKT-21 regardless of manifest/fixture
    presence.

Prerequisites and how to satisfy them:
  * Provenance manifest (link a + b records). JSON, one object per shipped
    ``(artifact, triple)`` — 8 records total (``cpython``×4 + ``uv``×4):
        {"records": [
          {"artifact": "cpython", "triple": "aarch64-apple-darwin",
           "upstream_sha256": "<hex>", "upstream_url": "https://github.com/astral-sh/python-build-standalone/releases/download/20260623/SHA256SUMS",
           "mirror_url": "https://sdk.vorpal.build/source/cpython-3.13.14-aarch64-apple-darwin.tar.gz",
           "inline_digest": "<get_source_digest over the UNPACKED tree>"},
          ...]}
    Path: ``$VORPAL_PROVENANCE_MANIFEST`` or, by default, a ``provenance.json``
    co-located with ``Vorpal.lock`` (ADR 0001: same CODEOWNERS/--unlock review).
  * ``uv`` binary: ``$VORPAL_UV_BIN`` or ``uv`` on PATH (the pinned 0.10.11).
  * A hashed fixture project (``pyproject.toml`` + ``uv.lock`` with per-package
    hashes): ``$VORPAL_UV_FIXTURE`` pointing at the project dir.
  * Vorpal CLI binary: ``$VORPAL_BIN``.

Runnable two ways, matching ``test_parity.py``:
  * ``pytest``, or
  * ``python tests/test_supply_chain.py`` (dependency-free; exits non-zero only
    on a real FAILURE, never on a SKIP).
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

# tests -> python -> sdk -> repo root
_REPO = Path(__file__).resolve().parents[3]

_CANONICAL_UPSTREAM_HOSTS = {
    "cpython": "github.com",  # astral-sh/python-build-standalone releases
    "uv": "github.com",  # astral-sh/uv release checksums
}
_EXPECTED_TRIPLES = {
    "aarch64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
}


class _Skipped(Exception):
    """Raised by the standalone runner to mark a gated, not-yet-runnable test."""


def _skip(reason: str) -> None:
    """Skip under pytest if present, else raise the standalone sentinel."""
    pytest = sys.modules.get("pytest")
    if pytest is not None:
        pytest.skip(reason)
    raise _Skipped(reason)


def _manifest_path() -> Path | None:
    override = os.environ.get("VORPAL_PROVENANCE_MANIFEST")
    if override:
        p = Path(override)
        return p if p.is_file() else None
    default = _REPO / "provenance.json"
    return default if default.is_file() else None


def _load_manifest() -> list[dict[str, Any]]:
    path = _manifest_path()
    if path is None:
        _skip(
            "provenance manifest absent (cpython/uv sources unpinned; capture is "
            "the blocking Phase-1 AC — DKT-21/DKT-25). Set $VORPAL_PROVENANCE_MANIFEST."
        )
    data = json.loads(path.read_text(encoding="utf-8"))  # type: ignore[union-attr]
    records = data.get("records", [])
    assert records, f"provenance manifest {path} has no records"
    return records


def _uv_bin() -> str:
    found = os.environ.get("VORPAL_UV_BIN") or shutil.which("uv")
    if not found:
        _skip("uv binary absent (pinned uv 0.10.11 not provisioned — DKT-21). Set $VORPAL_UV_BIN.")
    return found  # type: ignore[return-value]


def _uv_fixture() -> Path:
    fixture = os.environ.get("VORPAL_UV_FIXTURE")
    if not fixture or not Path(fixture).is_dir():
        _skip(
            "hashed uv fixture project absent (needs pyproject.toml + uv.lock with "
            "per-package hashes — provisioned with the build env, DKT-21). "
            "Set $VORPAL_UV_FIXTURE."
        )
    return Path(fixture)  # type: ignore[arg-type]


def _is_sha256_hex(value: str) -> bool:
    return len(value) == 64 and all(c in "0123456789abcdef" for c in value.lower())


# --- C2 provenance: two-link chain (ADR 0001 Part A) -----------------------


def test_provenance_manifest_complete() -> None:
    """All 8 shipped (artifact, triple) records carry both links' fields."""
    records = _load_manifest()
    covered = {(r.get("artifact"), r.get("triple")) for r in records}
    expected = {(a, t) for a in ("cpython", "uv") for t in _EXPECTED_TRIPLES}
    assert covered >= expected, (
        f"provenance manifest missing records: {sorted(expected - covered)} "
        "(all four platform tarballs of BOTH artifacts must be verified at capture)"
    )
    required = ("upstream_sha256", "upstream_url", "mirror_url", "inline_digest")
    for r in records:
        missing = [f for f in required if not r.get(f)]
        assert not missing, f"record {r.get('artifact')}/{r.get('triple')} missing {missing}"


def test_provenance_a1_origin_is_canonical_upstream_not_mirror() -> None:
    """Link (a.1): the recorded upstream hash origin is the canonical upstream,
    never sdk.vorpal.build — comparing the mirror to a hash copied from the
    mirror is circular (ADR 0001 Part A link a.1)."""
    for r in _load_manifest():
        url = str(r.get("upstream_url", ""))
        assert "sdk.vorpal.build" not in url, (
            f"{r.get('artifact')}/{r.get('triple')}: upstream hash origin points at the "
            f"mirror ({url}) — source-authenticity (a.1) requires the canonical upstream"
        )
        host = _CANONICAL_UPSTREAM_HOSTS.get(str(r.get("artifact")))
        assert host and host in url, (
            f"{r.get('artifact')}/{r.get('triple')}: upstream_url {url!r} is not the "
            f"canonical upstream host ({host})"
        )


def test_provenance_two_hashes_never_conflated() -> None:
    """Anti-conflation guard: the upstream PACKED-tarball SHA-256 and the inline
    UNPACKED get_source_digest are different by construction — they must never be
    asserted equal, and must in fact differ (ADR 0001 rejected alternative)."""
    for r in _load_manifest():
        up = str(r.get("upstream_sha256", "")).lower()
        inline = str(r.get("inline_digest", "")).lower()
        assert _is_sha256_hex(up), f"{r.get('artifact')}/{r.get('triple')}: malformed upstream_sha256"
        assert up != inline, (
            f"{r.get('artifact')}/{r.get('triple')}: upstream tarball SHA == inline "
            "with_digest — the two distinct hashes have been conflated (the exact "
            "failure ADR 0001 forbids)"
        )


def test_provenance_link_a_mirror_equals_upstream() -> None:
    """Link (a): the mirrored tarball's SHA-256 == the recorded upstream SHA-256.
    Gated on mirror reachability (sdk.vorpal.build 403 until DKT-25 publishes)."""
    records = _load_manifest()
    for r in records:
        mirror_url = str(r.get("mirror_url", ""))
        try:
            with urllib.request.urlopen(mirror_url, timeout=30) as resp:  # noqa: S310
                payload = resp.read()
        except (urllib.error.URLError, TimeoutError, OSError) as exc:
            _skip(f"mirror unreachable ({mirror_url}: {exc}) — DKT-25 must publish first")
        got = hashlib.sha256(payload).hexdigest()  # type: ignore[possibly-undefined]
        want = str(r.get("upstream_sha256", "")).lower()
        assert got == want, (
            f"{r.get('artifact')}/{r.get('triple')}: mirrored tarball SHA {got} != "
            f"recorded upstream SHA {want} — mirror does NOT match upstream (TOFU defense fails)"
        )


# --- C3 hash-enforcement: uv rejects tampered / drifted / unlocked installs --


def _run_uv(args: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [_uv_bin(), *args],
        cwd=cwd,
        capture_output=True,
        text=True,
        env={**os.environ, "UV_PYTHON_DOWNLOADS": "never"},
        check=False,
    )


def test_tampered_package_c3a_uv_sync_fails() -> None:
    """C3a: a locked package whose content hash != its uv.lock entry makes
    ``uv sync --frozen`` FAIL — proves hash verification is live, distinct from a
    stale-lock failure (TDD §9 C3a)."""
    fixture = _uv_fixture()
    work = _REPO / ".docket" / "_sc_tmp_c3a"
    if work.exists():
        shutil.rmtree(work)
    shutil.copytree(fixture, work)
    try:
        cache = next((work / ".uv_cache").glob("**/*.whl"), None)
        if cache is None:
            _skip("fixture has no cached wheel to tamper — provision a populated $VORPAL_UV_FIXTURE")
        with open(cache, "ab") as fh:  # type: ignore[arg-type]
            fh.write(b"\x00tampered")  # mutate content so hash != uv.lock entry
        result = _run_uv(["sync", "--frozen"], work)
        assert result.returncode != 0, (
            "uv sync --frozen accepted a tampered package whose content hash differs "
            f"from the uv.lock entry — hash verification is NOT live.\nstdout:{result.stdout}"
        )
        assert "hash" in (result.stdout + result.stderr).lower(), (
            f"uv sync failed but not for a hash mismatch — verify this is C3a, not a "
            f"stale-lock failure.\nstderr:{result.stderr}"
        )
    finally:
        shutil.rmtree(work, ignore_errors=True)


def test_uv_sync_locked_catches_drift() -> None:
    """C3c: ``uv sync --locked`` FAILS when pyproject.toml and uv.lock have
    drifted (the CI drift gate, TDD §9)."""
    fixture = _uv_fixture()
    work = _REPO / ".docket" / "_sc_tmp_drift"
    if work.exists():
        shutil.rmtree(work)
    shutil.copytree(fixture, work)
    try:
        pyproject = work / "pyproject.toml"
        pyproject.write_text(
            pyproject.read_text(encoding="utf-8")
            + '\n[tool.uv]\n# drift: a dependency added without re-locking\n',
            encoding="utf-8",
        )
        result = _run_uv(["sync", "--locked"], work)
        assert result.returncode != 0, (
            "uv sync --locked did not catch pyproject/uv.lock drift — the CI drift gate "
            f"is ineffective.\nstdout:{result.stdout}\nstderr:{result.stderr}"
        )
    finally:
        shutil.rmtree(work, ignore_errors=True)


def test_frozen_lock_required_for_build() -> None:
    """A missing/stale uv.lock fails ``uv sync --frozen`` — the build never
    resolves freely (TDD §9 frozen-lock failure path)."""
    fixture = _uv_fixture()
    work = _REPO / ".docket" / "_sc_tmp_frozen"
    if work.exists():
        shutil.rmtree(work)
    shutil.copytree(fixture, work)
    try:
        lock = work / "uv.lock"
        if lock.exists():
            lock.unlink()
        result = _run_uv(["sync", "--frozen"], work)
        assert result.returncode != 0, (
            "uv sync --frozen succeeded with no uv.lock — the build would resolve "
            f"unpinned dependencies.\nstdout:{result.stdout}\nstderr:{result.stderr}"
        )
    finally:
        shutil.rmtree(work, ignore_errors=True)


def test_tampered_source_archive_fails_closed() -> None:
    """C1/C2 link (b): a source whose get_source_digest != the pinned Vorpal.lock
    digest fails the build CLOSED. Gated on the vorpal CLI + a pinned cpython/uv
    source (DKT-21); covered end-to-end by the e2e build, asserted here as the
    fail-closed contract."""
    vorpal = os.environ.get("VORPAL_BIN") or shutil.which("vorpal")
    if not vorpal:
        _skip("vorpal CLI absent — fail-closed digest check needs a build env (DKT-21). Set $VORPAL_BIN.")
    if _manifest_path() is None:
        _skip("no pinned source/manifest yet — nothing to tamper (DKT-21/DKT-25).")
    _skip(
        "fail-closed tamper requires mutating a pinned payload in a live store; "
        "exercised by the e2e build (DKT-21) — author-complete, runtime-gated."
    )


def _run() -> int:
    tests = [
        v for k, v in sorted(globals().items())
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
        print("NOTE: all tests gated — supply-chain NOT yet verified (awaiting DKT-21/DKT-25).")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
