"""Offline checks that the linux-vorpal stage scripts match the book map.

``script/source/ledger.json`` maps every "##"/"###" block of the stage scripts
(``sdk/rust/src/artifact/linux_vorpal/script``) to the LFS/BLFS sections it
implements, or to a local reason, and records each block's hash at its last
review. ``script/source/book`` holds the reviewed commands of every mapped
section. These tests fail when a stage block changes, appears, disappears, or
moves without ``script/source.sh book-diff --mark``/``--map``, or when the map
and snapshots disagree. ``book-diff --check`` adds the online checks against
the tracked book.

The block rules repeat ``stage_blocks`` in ``script/source.sh``; change both
together.

Runnable with ``pytest`` or ``python tests/test_book_map.py``.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

# tests -> python -> sdk -> repo root
_REPO = Path(__file__).resolve().parents[3]
_LEDGER = _REPO / "script" / "source" / "ledger.json"
_SNAPSHOTS = _REPO / "script" / "source" / "book"
_SCRIPTS = _REPO / "sdk" / "rust" / "src" / "artifact" / "linux_vorpal" / "script"
_FILES = ["setup", "stage_01", "stage_02", "stage_03", "stage_04", "stage_05"]
_STEPS = _REPO / "sdk" / "rust" / "src" / "artifact" / "linux_vorpal.rs"

_HEADING = re.compile(r"^[ \t]*###? ")
_TERMINATOR = re.compile(r'[^\\]",?$')


def _ledger() -> dict[str, Any]:
    return json.loads(_LEDGER.read_text())


def _sha(lines: list[str]) -> str:
    return hashlib.sha256("".join(line + "\n" for line in lines).encode()).hexdigest()


def _stage_blocks() -> list[dict[str, str]]:
    blocks: list[dict[str, str]] = []
    for name in _FILES:
        started = done = False
        head = "(preamble)"
        lines: list[str] = []

        def flush() -> None:
            while lines and lines[-1] == "":
                lines.pop()
            if lines:
                blocks.append({"file": name, "heading": head, "sha": _sha(lines)})
            lines.clear()

        for raw in (_SCRIPTS / f"{name}.rs").read_text().split("\n"):
            if not started:
                started = 'formatdoc! {"' in raw
                continue
            line = raw.rstrip(" \t")
            if _HEADING.match(line):
                flush()
                head = re.sub(r"^[ \t]*#+[ \t]*", "", line)
            lines.append(line)
            if _TERMINATOR.search(line):
                flush()
                done = True
                break
        assert done, f"{name}.rs: no terminated formatdoc! body"

    # The step sequence of linux_vorpal.rs: from "let step_environments" to
    # the line before 'let name = "linux-vorpal";'.
    steps: list[str] = []
    inside = done = False
    for raw in _STEPS.read_text().split("\n"):
        if "let step_environments" in raw:
            inside = True
        if inside and 'let name = "linux-vorpal";' in raw:
            done = True
            break
        if inside:
            steps.append(raw.rstrip(" \t"))
    assert done, "linux_vorpal.rs: no step sequence"
    while steps and steps[-1] == "":
        steps.pop()
    blocks.append({"file": "linux_vorpal", "heading": "(steps)", "sha": _sha(steps)})
    return blocks


def _key(block: dict[str, Any]) -> str:
    return f"{block['file']}: {block['heading']}"


def test_stage_blocks_match_ledger() -> None:
    current = _stage_blocks()
    recorded = _ledger()["stage_blocks"]
    keys = [_key(b) for b in current]
    dups = sorted({k for k in keys if keys.count(k) > 1})
    assert not dups, f"duplicate stage block headings: {dups}"

    new = [k for k in keys if k not in {_key(b) for b in recorded}]
    gone = [_key(b) for b in recorded if _key(b) not in set(keys)]
    assert not new, f"new stage blocks, map them with book-diff --map: {new}"
    assert not gone, f"stage blocks gone, record with book-diff --mark (a section, or local:FILE:HEADING): {gone}"
    assert keys == [_key(b) for b in recorded], "stage blocks were reordered"

    changed = [
        _key(c) for c, r in zip(current, recorded) if c["sha"] != r["sha"]
    ]
    assert not changed, (
        "stage blocks changed since their last book-diff --mark "
        f"(review against script/source/book, then mark): {changed}"
    )


def test_every_block_and_section_is_mapped() -> None:
    ledger = _ledger()
    sections = {s["id"] for s in ledger["book_sections"]}
    problems = []
    referenced: set[str] = set()
    for block in ledger["stage_blocks"]:
        ids = block.get("sections") or []
        if not ids and not (block.get("local") or "").strip():
            problems.append(f"{_key(block)} maps to no section and has no local reason")
        for sid in ids:
            referenced.add(sid)
            if sid not in sections:
                problems.append(f"{_key(block)} names unmapped section {sid}")
    problems += [f"{sid} maps to no stage block" for sid in sorted(sections - referenced)]
    for section in ledger["book_sections"]:
        if section["status"] == "unreconciled":
            problems.append(f"{section['id']} is unreconciled; review and --mark it")
        if section.get("disposition") == "deviation" and not (section.get("reason") or "").strip():
            problems.append(f"{section['id']} is a deviation without a reason")
    for skip in ledger["book_skipped"]:
        if skip.get("status") == "unreconciled":
            problems.append(f"{skip['id']}: skipped section changed; confirm with --skip or map it")
    if ledger["lfs"]["tracked"] != ledger["blfs"]["tracked"]:
        problems.append("lfs.tracked and blfs.tracked differ")
    skipped = {s.get("id") for s in ledger["book_skipped"]}
    problems += [f"{sid} is both mapped and skipped" for sid in sorted(sections & skipped)]
    assert not problems, "\n".join(problems)


def test_snapshots_match_ledger() -> None:
    ledger = _ledger()
    tracked = ledger["lfs"]["tracked"]
    problems = []
    for section in ledger["book_sections"] + [s for s in ledger["book_skipped"] if s.get("id")]:
        book, page = section["id"].split(":", 1)
        path = _SNAPSHOTS / book / f"{page}.sh"
        if not path.is_file():
            problems.append(f"{section['id']}: no snapshot at {path.relative_to(_REPO)}")
            continue
        lines = path.read_text().split("\n")
        if f" {tracked}: " not in lines[0]:
            problems.append(f"{section['id']}: snapshot is not from {tracked}")
        body = "\n".join(lines[4:]).encode()
        if hashlib.sha256(body).hexdigest() != section["sha"]:
            problems.append(f"{section['id']}: snapshot does not match the ledger sha")
    assert not problems, "\n".join(problems)


def _run() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_") and callable(v)]
    failures = 0
    for t in tests:
        try:
            t()
            print(f"PASS {t.__name__}")
        except Exception as exc:  # noqa: BLE001 - runner surfaces any failure
            failures += 1
            print(f"FAIL {t.__name__}: {exc}")
    print(f"\n{len(tests) - failures} passed, {failures} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
