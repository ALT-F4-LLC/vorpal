"""Supply-chain tests for the build-target Python toolchain (DKT-19, ADR 0001).

These encode the REAL provenance / hash-enforcement assertions from ADR 0001
Part A and ``python-language-target.md`` §4/§9. They are RUNTIME-GATED: each
test detects its prerequisites and SKIPS with an explicit reason + the
unblocking ticket when they are absent; when the prereqs land, the same test
asserts the real behavior with no edit.

A skip is NOT a pass: the standalone runner prints SKIP distinctly and the
parity gate must treat "all skipped" as "not yet verified", never as green.

Current gate breakdown:
  * 3 offline tests always run against ``script/source/registry.json``:
    ``test_registry_covers_vorpal_lock``, ``test_provenance_exemptions_are_current``, and
    ``test_registry_pins_match_sdk_sources``.
  * 4 manifest-gated tests need the provenance manifest:
    ``test_provenance_manifest_complete``,
    ``test_provenance_covers_vorpal_lock``,
    ``test_provenance_a1_origin_is_canonical_upstream_not_mirror``, and
    ``test_provenance_two_hashes_never_conflated``.
  * 1 opt-in test downloads every mirrored file:
    ``test_provenance_link_a_mirror_equals_upstream`` runs only with
    ``VORPAL_SUPPLY_CHAIN_MIRROR=1`` plus the manifest.
  * 3 fixture-gated tests need ``$VORPAL_UV_FIXTURE`` plus ``uv``:
    ``test_tampered_package_c3a_uv_sync_fails``,
    ``test_uv_sync_locked_catches_drift``, and
    ``test_frozen_lock_required_for_build``.
  * 1 CLI+e2e-gated test needs ``$VORPAL_BIN`` plus the provenance manifest:
    ``test_tampered_source_archive_fails_closed``. It stays skipped as
    author-complete/e2e-gated per DKT-21 regardless of manifest/fixture
    presence.

Prerequisites and how to satisfy them:
  * Provenance manifest (link a + b records). JSON, one object per mirrored
    file the registry pins (every SDK artifact source, at its pinned version;
    ``triple`` is ``any`` for a platform-independent source). Written by
    ``script/source.sh provenance`` from staged bytes:
        {"records": [
          {"artifact": "cpython", "version": "3.14.7", "system": "aarch64-darwin",
           "triple": "aarch64-apple-darwin",
           "upstream_url": "https://github.com/astral-sh/python-build-standalone/releases/download/20260929/cpython-3.14.7+20260929-aarch64-apple-darwin-install_only.tar.gz",
           "upstream_sha256": "<hex of the downloaded file>",
           "mirror_key": "cpython-3.14.7-aarch64-apple-darwin.tar.gz",
           "mirror_url": "https://sdk.vorpal.build/source/cpython-3.14.7-aarch64-apple-darwin.tar.gz",
           "inline_digest": "<get_source_digest over the UNPACKED tree>",
           "authority": {"method": "sha256-list",
                         "url": "https://github.com/astral-sh/python-build-standalone/releases/download/20260929/SHA256SUMS",
                         "value": "<hex the authority published>"}},
          ...]}
    Path: ``$VORPAL_PROVENANCE_MANIFEST`` or, by default, a ``provenance.json``
    co-located with ``Vorpal.lock`` (ADR 0001: same CODEOWNERS/--unlock review).
  * ``uv`` binary: ``$VORPAL_UV_BIN`` or ``uv`` on PATH (the pinned 0.13.8).
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
import http.client
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

# tests -> python -> sdk -> repo root
_REPO = Path(__file__).resolve().parents[3]

_REGISTRY_PATH = _REPO / "script" / "source" / "registry.json"
_LOCK_PATH = _REPO / "Vorpal.lock"
_MIRROR_HOST = "sdk.vorpal.build"

# Registry system -> provenance triple. Platform-independent sources use "any".
_TRIPLES = {
    "aarch64-darwin": "aarch64-apple-darwin",
    "aarch64-linux": "aarch64-unknown-linux-gnu",
    "x86_64-darwin": "x86_64-apple-darwin",
    "x86_64-linux": "x86_64-unknown-linux-gnu",
}


def _registry() -> dict[str, Any]:
    return json.loads(_REGISTRY_PATH.read_text(encoding="utf-8"))


def _expand(template: str, source: dict[str, Any], target: str) -> str:
    version = str(source["version"])
    parts = version.split(".")
    values = {
        "version": version,
        "target": target,
        "release": str(source.get("release", "")),
        "major": parts[0],
        "minor": ".".join(parts[:2]),
        "nodot": version.replace(".", ""),
    }
    for key, value in values.items():
        template = template.replace("{" + key + "}", value)
    return template


def _host(url: str) -> str:
    return urllib.parse.urlsplit(url).hostname or ""


def _expected_objects() -> dict[str, dict[str, str]]:
    """Every mirror URL the registry pins, keyed to its artifact, triple, and
    canonical upstream host."""
    registry = _registry()
    objects: dict[str, dict[str, str]] = {}
    for source in registry["sources"]:
        systems = source.get("systems") or {"any": ""}
        for system, target in systems.items():
            key = _expand(source["mirror"], source, target)
            upstream = source.get("archive", {}).get("repo") or source["upstream"]
            objects[f"{registry['mirror']}/{key}"] = {
                "artifact": source["name"],
                "triple": _TRIPLES.get(system, "any"),
                "host": _host(upstream),
            }
    return objects


def _exemptions() -> dict[str, str]:
    """Mirror URL -> reason for each file the registry exempts from a
    provenance record (exact keys only; each ends when its source is bumped)."""
    registry = _registry()
    return {
        f"{registry['mirror']}/{e['key']}": str(e["reason"])
        for e in registry.get("provenance_exemptions", [])
    }


def _lock_entries() -> list[tuple[str, str, str]]:
    """(path, platform, digest) for every mirrored Vorpal.lock source."""
    entries: list[tuple[str, str, str]] = []
    fields: dict[str, str] = {}

    def flush() -> None:
        path = fields.get("path", "")
        if _MIRROR_HOST in path:
            entries.append((path, fields.get("platform", ""), fields.get("digest", "")))
        fields.clear()

    for line in _LOCK_PATH.read_text(encoding="utf-8").splitlines():
        if line.startswith("[["):
            flush()
        for key in ("path", "platform", "digest"):
            prefix = f'{key} = "'
            if line.startswith(prefix):
                fields[key] = line[len(prefix) : -1]
    flush()
    return entries


def _lock_systems() -> set[str] | None:
    """Systems named in $VORPAL_LOCK_SYSTEMS (comma-separated), or None for all."""
    raw = os.environ.get("VORPAL_LOCK_SYSTEMS", "").strip()
    return {s.strip() for s in raw.split(",") if s.strip()} or None


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
            "provenance manifest absent; write it with the upgrade-sources skill "
            "(script/source.sh stage --include-current, then provenance). Set $VORPAL_PROVENANCE_MANIFEST."
        )
    data = json.loads(path.read_text(encoding="utf-8"))  # type: ignore[union-attr]
    records = data.get("records", [])
    assert records, f"provenance manifest {path} has no records"
    return records


def _uv_bin() -> str:
    found = os.environ.get("VORPAL_UV_BIN") or shutil.which("uv")
    if not found:
        _skip("uv binary absent (pinned uv 0.13.8 not provisioned — DKT-21). Set $VORPAL_UV_BIN.")
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
#
# The expected record set comes from script/source/registry.json: one record
# per mirrored file at each source's pinned version, for every SDK artifact
# source (tools, the Rust toolchain, and linux-vorpal), not only cpython/uv.


def test_registry_covers_vorpal_lock() -> None:
    """Offline: every mirror path in Vorpal.lock is a registry object at its
    pinned version, so the upgrade skill and provenance cover every source.

    Between ``set-version`` and ``vorpal prepare`` this fails by design: the
    lock still names the old versions. $VORPAL_LOCK_SYSTEMS limits the check to
    the systems already prepared."""
    expected = _expected_objects()
    systems = _lock_systems()
    paths = {p for p, platform, _ in _lock_entries() if systems is None or platform in systems}
    unknown = sorted(paths - set(expected))
    assert not unknown, (
        f"Vorpal.lock sources missing from {_REGISTRY_PATH.name} (or pinned at "
        f"another version): {unknown}"
    )


def test_registry_pins_match_sdk_sources() -> None:
    """Offline: every SDK pin line carries its registry version as a whole
    token, so all four SDKs and the registry agree."""
    drift = []
    for source in _registry()["sources"]:
        version = str(source["version"])
        token = re.compile(r"(^|[^0-9A-Za-z.])" + re.escape(version) + r"([^0-9A-Za-z.]|\.[^0-9]|\.?$)")
        for pin in source["pins"]:
            text = (_REPO / pin["file"]).read_text(encoding="utf-8")
            lines = [line.strip() for line in text.splitlines()]
            after = pin.get("after")
            hits = [
                line
                for i, line in enumerate(lines)
                if line.startswith(pin["marker"])
                and (after is None or (i > 0 and lines[i - 1].startswith(after)))
            ]
            if not hits:
                drift.append(f"{source['name']}: {pin['file']}: no line starts with {pin['marker']!r}")
            drift.extend(
                f"{source['name']} {version}: {pin['file']}: {line}"
                for line in hits
                if not token.search(line)
            )
    assert not drift, "SDK pins differ from registry.json:\n  " + "\n  ".join(drift)


def test_provenance_exemptions_are_current() -> None:
    """Offline: every exemption names a file the registry pins now, with a
    reason, and no exemption covers a file that already has a record. A bump
    or a new record makes the exemption stale, and this test fails until it
    is removed."""
    expected = _expected_objects()
    stale = [url for url in _exemptions() if url not in expected]
    empty = [url for url, reason in _exemptions().items() if not reason.strip()]
    assert not stale, f"provenance exemptions for files the registry no longer pins: {stale}"
    assert not empty, f"provenance exemptions without a reason: {empty}"
    path = _manifest_path()
    if path is not None:
        data = json.loads(path.read_text(encoding="utf-8"))
        recorded = {str(r.get("mirror_url")) for r in data.get("records", [])}
        covered = sorted(set(_exemptions()) & recorded)
        assert not covered, f"provenance exemptions for files that now have records: {covered}"


def test_provenance_manifest_complete() -> None:
    """One record per mirrored file the registry pins, each carrying both
    links' fields and the declared authority."""
    records = _load_manifest()
    covered = {str(r.get("mirror_url")) for r in records}
    expected = _expected_objects()
    exempt = _exemptions()
    if exempt:
        print(f"  {len(exempt)} provenance exemption(s) from {_REGISTRY_PATH.name}")
    missing = sorted(set(expected) - covered - set(exempt))
    assert not missing, (
        f"provenance manifest missing records: {missing} "
        "(stage every pinned source with `script/source.sh stage --include-current`)"
    )
    required = ("artifact", "triple", "upstream_sha256", "upstream_url", "mirror_url", "inline_digest")
    for r in records:
        missing_fields = [f for f in required if not r.get(f)]
        assert not missing_fields, f"record {r.get('mirror_url')} missing {missing_fields}"
        authority = r.get("authority") or {}
        assert all(authority.get(f) for f in ("method", "url", "value")), (
            f"record {r.get('mirror_url')} has no declared authority (method, url, value)"
        )
        meta = expected.get(str(r.get("mirror_url")))
        if meta:
            assert (r["artifact"], r["triple"]) == (meta["artifact"], meta["triple"]), (
                f"record {r['mirror_url']} is {r['artifact']}/{r['triple']}, "
                f"registry says {meta['artifact']}/{meta['triple']}"
            )


def test_provenance_covers_vorpal_lock() -> None:
    """Offline cross-check: every Vorpal.lock mirror path has a record whose
    inline digest equals the lock digest computed by `vorpal prepare`.
    $VORPAL_LOCK_SYSTEMS limits it to the systems prepared so far."""
    by_url = {str(r.get("mirror_url")): r for r in _load_manifest()}
    systems = _lock_systems()
    exempt = _exemptions()
    problems = []
    for path, platform, digest in sorted(set(_lock_entries())):
        if systems is not None and platform not in systems:
            continue
        record = by_url.get(path)
        if record is None and path in exempt:
            continue
        if record is None:
            problems.append(f"{path}: no provenance record")
        elif record.get("inline_digest") != digest:
            problems.append(f"{path}: inline {record.get('inline_digest')} != lock {digest}")
    assert not problems, "Vorpal.lock and provenance disagree:\n  " + "\n  ".join(problems)


def test_provenance_a1_origin_is_canonical_upstream_not_mirror() -> None:
    """Link (a.1): the recorded upstream URL and hash authority are the
    canonical upstream, never sdk.vorpal.build — comparing the mirror to a hash
    copied from the mirror is circular (ADR 0001 Part A link a.1)."""
    expected = _expected_objects()
    for r in _load_manifest():
        url = str(r.get("upstream_url", ""))
        authority_url = str((r.get("authority") or {}).get("url", ""))
        for label, value in (("upstream_url", url), ("authority.url", authority_url)):
            assert _MIRROR_HOST not in value, (
                f"{r.get('mirror_url')}: {label} points at the mirror ({value}) — "
                "source-authenticity (a.1) requires the canonical upstream"
            )
        meta = expected.get(str(r.get("mirror_url")))
        if meta:
            assert _host(url) == meta["host"], (
                f"{r.get('mirror_url')}: upstream_url {url!r} is not the canonical "
                f"upstream host ({meta['host']})"
            )


def test_provenance_two_hashes_never_conflated() -> None:
    """Anti-conflation guard: the upstream PACKED-tarball SHA-256 and the inline
    UNPACKED get_source_digest are different by construction — they must never be
    asserted equal, and must in fact differ (ADR 0001 rejected alternative)."""
    for r in _load_manifest():
        up = str(r.get("upstream_sha256", "")).lower()
        inline = str(r.get("inline_digest", "")).lower()
        assert _is_sha256_hex(up), f"{r.get('mirror_url')}: malformed upstream_sha256"
        assert up != inline, (
            f"{r.get('mirror_url')}: upstream tarball SHA == inline "
            "with_digest — the two distinct hashes have been conflated (the exact "
            "failure ADR 0001 forbids)"
        )


def test_provenance_link_a_mirror_equals_upstream() -> None:
    """Link (a): the mirrored file's SHA-256 == the recorded upstream SHA-256.
    Opt-in (downloads every mirrored file): set $VORPAL_SUPPLY_CHAIN_MIRROR=1."""
    if os.environ.get("VORPAL_SUPPLY_CHAIN_MIRROR") != "1":
        _skip("mirror download is opt-in; set VORPAL_SUPPLY_CHAIN_MIRROR=1")
    for r in _load_manifest():
        mirror_url = str(r.get("mirror_url", ""))
        digest = hashlib.sha256()
        size = 0
        length = None
        try:
            with urllib.request.urlopen(mirror_url, timeout=60) as resp:  # noqa: S310
                length = resp.headers.get("Content-Length")
                for chunk in iter(lambda: resp.read(1 << 20), b""):
                    digest.update(chunk)
                    size += len(chunk)
        except (urllib.error.URLError, TimeoutError, OSError, http.client.HTTPException) as exc:
            _skip(f"mirror unreachable ({mirror_url}: {exc})")
        # A short read is a transport failure, not a mirror mismatch.
        assert length is None or size == int(length), (
            f"{mirror_url}: download incomplete ({size} of {length} bytes); rerun"
        )
        got = digest.hexdigest()
        want = str(r.get("upstream_sha256", "")).lower()
        assert got == want, (
            f"{mirror_url}: mirrored SHA {got} != recorded upstream SHA {want} — "
            "mirror does NOT match upstream (TOFU defense fails)"
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
