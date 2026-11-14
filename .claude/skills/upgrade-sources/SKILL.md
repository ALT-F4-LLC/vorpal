---
name: upgrade-sources
description: "Upgrade SDK artifact sources (tools, the Rust toolchain, linux-vorpal) to their latest stable versions across the Rust, Go, TypeScript, and Python SDKs, and stage verified sources for the sdk.vorpal.build/source mirror. Use on \"upgrade sources\", \"bump the tools\", \"update linux-vorpal to the new LFS book\", \"/upgrade-sources --group go\"."
argument-hint: "[--kind tool|rust|linux-vorpal] [--not-kind KIND] [--group GROUP] [--name NAME] [--not-name NAME] [--target-version V]"
---

# Upgrade sources

Moves every pinned upstream source behind the SDK artifacts to its target
version, in all four SDKs at once, and stages verified copies for the operator
to upload to `https://sdk.vorpal.build/source/`.

`script/source.sh` does the mechanical work. This skill owns the order, the
gates, and the decisions. Arguments to the skill are filters, passed to every
`script/source.sh` call that takes them.

## Files

| File | Role |
| --- | --- |
| `script/source/registry.json` | One entry per source: kind, groups, version, track, discovery method, systems, upstream and mirror templates, declared authority (`verify[0]`) and stated fallbacks, SDK pins, affected `vorpal*` artifacts. |
| `script/source/ledger.json` | Tracked LFS/BLFS book versions, each deviation from the books with its reason, and the book map: `book_sections` (mapped sections in book order, with the hash of their reviewed commands, `status`, `disposition`, `reason`), `book_skipped` (sections not implemented, with reasons), and `stage_blocks` (every `##`/`###` block of the stage scripts in order, with its hash and its sections or local reason). |
| `script/source/book/` | The reviewed commands of every mapped section, one file per section, extracted under the books' MIT License (`LICENSE` there). |
| `script/source/keys/<name>.gpg` | Operator-pinned keyrings; `registry.json` `.keys.<name>.sha256` pins each file. |
| `script/source.sh` | `check`, `stage`, `mirror-check`, `upload-cmd`, `provenance`, `pins`, `set-version`, `digest`, `digest-fill`, `book-diff`. Run `script/source.sh help`. |
| `.source-stage/` | Staging tree (gitignored): `plan.json`, `objects/<key>`, `manifest.json`, `mirror-status.json`, `present.ok`. It is the run's state. |
| `provenance.json` | One record per mirrored file, next to `Vorpal.lock`. |

## Filters

Filters combine; a source must match all of them.

- `--kind tool|rust|linux-vorpal` (repeatable), `--not-kind KIND`
- `--group go|rust|typescript|python|oci|vcs|protobuf|system`
- `--name NAME` (repeatable), `--not-name NAME` (repeatable)
- `--target-version V` with `--name` for every source in one pin-sharing group
  (glibc needs `--name glibc --name glibc-patch`). Book-tracked sources also
  need `--advisory URL`, an `https://www.linuxfromscratch.org/lfs/advisories/`
  or `/blfs/advisories/` page. Rust sources refuse an override.

Examples: everything except linux-vorpal is `--not-kind linux-vorpal`; only Go
sources is `--kind tool --group go`; the Rust toolchain is `--kind rust`.

The book set (LFS and BLFS sources) moves as one. `check` refuses a
selection that holds some book sources but not all of them.

## Version policy

Each source's `track` decides its target. Never pick a release candidate,
beta, alpha, nightly, or draft.

| Track | Target |
| --- | --- |
| `newest` | Newest stable upstream release, including a new major line. No per-line question. |
| `lts` | Newest LTS release (Node.js). |
| `rust-toolchain.toml` | The `channel` in `rust-toolchain.toml`. The seven Rust components never take another version. |
| `book` | The version in the current LFS stable-systemd book (`lfs`), else the BLFS stable-systemd book (`blfs`), else upstream (`upstream`, recorded in the ledger). |
| `lfs:python` | cpython follows the LFS book's Python version exactly, from the newest python-build-standalone release that ships it for all four systems. |

Rules that hold for every track:

- A book-tracked source goes above the book only when an LFS or BLFS security
  advisory names the fixed version. Pass `--target-version` with
  `--advisory URL`, and record a ledger deviation.
- Bump only when upstream ships every declared system (`systems` in the
  registry; a tag must exist for tag-archive sources). `check` marks the rest
  `blocked`.
- All four SDKs move together or none does.
- Never downgrade. `check` marks an older target `held`.
- Patches come only from the tracked book's Needed Patches
  (`https://www.linuxfromscratch.org/patches/`). A patch that is not there is
  a ledger deviation the operator approves.

## Verification

`verify[0]` is the declared authority. `stage` uses it alone and stops when it
is missing or fails. Later entries are the stated fallback; using one takes an
operator decision and a registry edit that moves it first. The order of
strength is: signature over a pinned keyring, then a publisher checksum file
over HTTPS, then the book MD5 (only where the book is the sole publisher).

Tag-only sources (crane, go-tools, grpcurl, protoc-gen-go-grpc, staticcheck)
are `git archive` at the tag, with the registry prefix, through `gzip -n`. The
commit must equal the GitHub tag ref or the Go proxy's `.info` `Origin.Hash`,
and the record keeps the commit and the tag ref as the authority.

Keyrings start unpinned. Before any download, `stage` checks every keyring
the selection needs and exits 4 naming each unpinned one. Fetch each, print
its fingerprints and its sha256, and ask the operator through one
`AskUserQuestion` whether to pin them. Write a pin only on approval.

## Flow

Stop at the first unexpected failure and report it. `.source-stage/` and the
working tree hold the state; a later session resumes at the first row whose
condition holds:

| State | Resume at |
| --- | --- |
| no `plan.json` | 2 |
| `plan.json`, no `manifest.json` (`check` and `stage` delete it) | 3, after asking whether the plan is still approved |
| `ledger.json` has an `unreconciled` section (after `book-diff --accept`) | 4, book reconciliation |
| `manifest.json`, `pins` still reports the old versions | 4 |
| pins moved, no `mirror-status.json` | 5, `mirror-check` |
| `manifest.json` has an empty `inline_digest` | 3, `digest-fill` |
| `mirror-status.json` records a `conflict` | stop: report the key and both hashes |
| `mirror-status.json` with `absent` keys, no `present.ok` | 5, ask whether the upload finished |
| every key `same`, or `present.ok`, and `provenance.json` lacks a manifest record | 5, `provenance` |
| `provenance.json` holds every manifest record, lock not prepared | 6 |

Never rerun `check` after `set-version`: the new plan is all `current` and
deletes the manifest. If the manifest is lost after step 4, rebuild it with
`check` then `stage --include-current` for the same filters. That does not
work for a bumped tag-archive source (crane, go-tools, grpcurl,
protoc-gen-go-grpc, staticcheck), which `--include-current` skips: revert its
`set-version` edits, then rerun `check` and `stage`.

### 1. Preflight

```bash
git status --short
script/source.sh pins
cargo run -q -p vorpal-sdk-codegen -- --check
script/source.sh book-diff --check
```

`pins` must report that every pin matches. Fix drift before planning.
`book-diff --check` must pass: a stage block edited without `--mark`, a block
or book section with no mapping, or a snapshot that no longer matches the
tracked book is drift too. Its "never audited" note is not a failure.

### 2. Dry run and approval

```bash
script/source.sh check <filters>
```

Show the report verbatim: artifact, kind, current, target, status, authority
URL. It writes only `.source-stage/plan.json`.

If `check` reports that the LFS book changed and the selection includes
linux-vorpal, run `script/source.sh book-diff` before asking (see Book
tracking). Add to the report its summary line, every changed section with its
command diff and the stage blocks it maps to, and every new, removed, or
reordered section. Each new section needs a choice in the same
`AskUserQuestion`: implement it (name the stage block; a package new to the
build also needs a registry source and SDK source helpers) or skip it with a
reason. Read each chapter 8 `info:` addition against the chapter 8 subset
deviation in `ledger.json`, and list it in the report.

A `blocked` row whose message names a URL usually means the upstream layout
moved, not that a system is missing: for example mpc 1.4 ships only `.tar.xz`,
and ncurses releases live in `archives/ncurses/` as `.tar.gz` while dated
snapshots were `current/*.tgz`. Confirm with the book's `wget-list-systemd`,
then fix the registry `upstream` template (and, when the file extension
changes, the `mirror` template, the SDK source helper in
`sdk/rust/src/artifact/linux_vorpal/source.rs`, and the ledger
archive-format deviation) and rerun `check`. A system that upstream truly
does not ship stays blocked. When a tool changes its packaging (pnpm 12 ships
`pnpm-darwin-arm64.tar.gz` where pnpm 10 shipped a bare `pnpm-macos-arm64`
binary), the bump also changes the artifact's build steps in all four SDKs;
list that in the report and leave the row blocked unless the operator
approves the larger change.

When the plan moves the LFS set to a book without UnZip while the UnZip
deviation in `ledger.json` is still open, add that choice (`bsdunzip` or a
recorded exception) to the same `AskUserQuestion`.

Ask once through `AskUserQuestion` for approval of the whole report. Nothing is
staged, edited, or built before that answer. A `blocked` or `held` row is
reported, never bumped.

### 3. Stage and verify

```bash
script/source.sh stage
```

`stage` skips `blocked` and `held` rows and prints them. Inside the book set
(LFS and BLFS sources) one blocked row holds every book bump, because the
set moves as one; the other rows still stage. Report each held row. `check` and `stage` delete
the previous `manifest.json`, so a failed stage never leaves a stale manifest
to upload from.

Exit 4: pin a keyring as above, then rerun. A source with paths that differ
only in case (the Linux kernel) gets an empty inline digest on macOS. Compute
it in Lima, which mounts the home directory read-only, and apply it on the
host before `provenance`:

```bash
limactl shell vorpal-aarch64 bash script/source.sh digest-fill --print --dir "$PWD/.source-stage" >"$TMPDIR/digests.txt"
script/source.sh digest-fill --apply "$TMPDIR/digests.txt"
```

Stage never downloads from sdk.vorpal.build.

### 4. Edit the SDKs

When the book set moves, first record the new sections (Book tracking, step
2) and run `script/source.sh book-diff --accept`, before any `set-version`.

For each bumped source:

```bash
script/source.sh set-version --name <name> --to <target> [--release <release>]
```

Sources that share pins move together (the Rust components, glibc and its
patch, unzip and its patches). For linux-vorpal, `set-version` edits only
`sdk/rust/src/artifact/linux_vorpal.rs`; then regenerate Go, TypeScript, and
Python:

```bash
cargo run -q -p vorpal-sdk-codegen
cargo run -q -p vorpal-sdk-codegen -- --check
```

Then finish the edits `set-version` cannot make:

- linux-vorpal stage scripts that hardcode a version (`grep -rn '<old>'
  sdk/rust/src/artifact/linux_vorpal/`; glibc appears in `setup.rs` and
  `stage_01.rs`).
- When the book set moves: the book reconciliation in Book tracking, steps
  4 to 6. It is mandatory; the run does not continue to step 5 until
  `book-diff --check` passes.
- A cpython line change (3.13 to 3.14): the rule text in the cpython
  artifact files, `.python-version`, `requires-python`, and
  `sdk/python/tests/test_interpreter_pin.py`.
- Aliases, doc comments, docs, and website text: `git grep -nF '<old>'` for
  each old version, and update every hit that names the bumped source. Known
  spots: the crane and rsync alias comments in `sdk/typescript/src/artifact.ts`
  and the uv HASH-ENFORCEMENT comments in `uv.go`, `uv.rs`, and `uv.ts`.
- `CHANGELOG.md` `[Unreleased]`: one line per bumped source, `old -> new`.
- `script/source/ledger.json`: each new or closed deviation (`book-diff
  --accept` moves the tracked book versions).
- `script/source/registry.json` `provenance_exemptions`: remove the entry for
  each bumped source.

Then regenerate again, since the stage-script edits must reach Go,
TypeScript, and Python too, and check:

```bash
cargo run -q -p vorpal-sdk-codegen
cargo run -q -p vorpal-sdk-codegen -- --check
script/source.sh pins
script/source.sh book-diff --check
python3 -I sdk/python/tests/test_book_map.py
```

When the selection includes kind `rust`, also run the drift test, which must
now pass:

```bash
cargo test -p vorpal-sdk-codegen --test rust_toolchain_drift
```

Without a `rust` selection the drift test is red whenever
`rust-toolchain.toml` is ahead of the SDK literals; that is expected, not a
stop, but say in the report that it also fails `make test`.

Do not run `test_supply_chain.py` yet: `test_registry_covers_vorpal_lock`
fails by design until `vorpal prepare` rewrites the lock (step 6).

### 5. Upload (operator)

```bash
script/source.sh mirror-check
```

`absent` keys need upload. `same` keys are already there. A `conflict` (exit
3) stops the run: report the key and both hashes, and never overwrite or
delete a mirror object. `mirror-check` downloads each object the mirror
already holds, in full, to compare bytes; this happens before upload and
before `vorpal prepare`, and the report says how many it fetched.

Ask the operator for the bucket name through `AskUserQuestion` unless they
gave it, then print the commands verbatim:

```bash
script/source.sh upload-cmd --bucket <bucket>
```

Each command is `aws s3api put-object ... --if-none-match '*'
--checksum-sha256 <base64>`, which fails rather than overwrite. Never use
`aws s3 cp` or `aws s3 sync`. The skill does not run the upload.

Pause, and ask through `AskUserQuestion` whether the upload finished. On
resume:

```bash
script/source.sh mirror-check --present
script/source.sh provenance
```

`--present` only asks whether each key exists; it never re-downloads mirror
objects. Exit 6 means a key is still missing: ask again. `provenance` writes
records only for keys that `mirror-check` found `same`, or found `absent`
and `--present` then confirmed; it refuses anything else.

### 6. Lock

`Vorpal.lock` changes only through `vorpal prepare`, which pins sources for
the host system only and needs the Vorpal services running on that system.
Run it for every affected `vorpal*` artifact with each of the four configs on
every locked system you can reach. `$VORPAL` is the freshly built binary:
`target/debug/vorpal` on the host, `~/vorpal/target/debug/vorpal` in Lima
(`lima-sync` builds it).

```bash
for config in "" Vorpal.go.toml Vorpal.ts.toml Vorpal.py.toml; do
  for artifact in <affected artifacts>; do
    "$VORPAL" prepare ${config:+--config "$config"} "$artifact"
  done
done
```

| System | Where |
| --- | --- |
| `aarch64-darwin` | this host |
| `aarch64-linux` | Lima `vorpal-aarch64` |
| `x86_64-linux` | Lima `vorpal-x86_64` |
| `x86_64-darwin` | not reachable here: CI's prepare job uploads `vorpal-lock-x86_64-darwin`; the operator replaces `Vorpal.lock` from it |

Lima works on a copy: `lima-sync` rsyncs the tree to `~/vorpal` in the VM
with `--delete`, and the host home is mounted read-only. Prepare the VMs one
after the other, so each starts from the lock the previous one wrote:

1. `make LIMA_ARCH=aarch64 lima-sync`
2. Start the services in the VM (`make LIMA_ARCH=aarch64 lima-vorpal-start`,
   in its own terminal).
3. Run the loop in `~/vorpal` through `limactl shell vorpal-aarch64`.
4. Copy the lock back: `limactl copy vorpal-aarch64:vorpal/Vorpal.lock Vorpal.lock`,
   and check with `git diff Vorpal.lock` that only `aarch64-linux` entries
   changed.
5. Repeat 1-4 with `x86_64`.

Quote every `mint:` and `update:` line `prepare` prints. List the systems
left unpinned. Then cross-check the lock against the staged digests for the
systems you prepared:

```bash
VORPAL_LOCK_SYSTEMS=aarch64-darwin,aarch64-linux,x86_64-linux python3 -I sdk/python/tests/test_supply_chain.py
```

`test_registry_covers_vorpal_lock` must pass for those systems.
`test_provenance_covers_vorpal_lock` must find no digest mismatch; it skips
exempt keys, and its only allowed failures are "no provenance record" for
blocked, held, or unpinned-keyring sources (see the baseline section). List
each one in the report. The unscoped run stays red until the
x86_64-darwin lock lands from CI; say so in the report.

### 7. Build and parity

Build each changed artifact. Build linux-vorpal in Lima on both
architectures:

```bash
make LIMA_ARCH=aarch64 VORPAL_ARTIFACT=linux-vorpal lima-vorpal
make LIMA_ARCH=x86_64 VORPAL_ARTIFACT=linux-vorpal lima-vorpal
```

Then run `/sdk-parity <artifact>` for each affected `vorpal*` artifact. The
registry's `parity` field lists them:

| Source | Affected artifacts |
| --- | --- |
| linux-vorpal (any) | all: `vorpal`, `vorpal-container-image`, `vorpal-job`, `vorpal-process`, `vorpal-shell`, `vorpal-user`, `vorpal-website` |
| Rust toolchain, protoc | `vorpal`, plus `vorpal-shell` for protoc |
| bun | `vorpal-shell`, `vorpal-website` |
| rsync | `vorpal-container-image`, `vorpal-shell` |
| every other tool | `vorpal-shell` |

`gh` also feeds `vorpal-release`, which the sdk-parity skill does not cover;
build it directly.

### 8. Report

Report: the approved table; what was staged, with each authority; the upload
commands; the `mint:`/`update:` lines and unpinned systems; build and parity
results; test output; ledger changes; and every check not run.

## Provenance baseline

`provenance.json` needs one record per mirrored file, including sources no
run has bumped. On the first run, also stage every current source:

```bash
script/source.sh check --not-name unzip-patch-fixes --not-name unzip-patch-gcc14
script/source.sh stage --include-current
```

then go through the mirror check and `provenance` as usual. For a current
source, `mirror-check` must say `same`. These cannot be baselined at their
current version and are reported instead:

- Tag-archive sources (crane, go-tools, grpcurl, protoc-gen-go-grpc,
  staticcheck): the existing mirror archives were packed by another gzip, so
  rebuilt bytes would conflict even when the inline digest matches
  `Vorpal.lock`. `stage --include-current` skips them; they get records when
  next bumped, as a new key.
- The UnZip patches: BLFS prints no MD5 for them, so their authority cannot
  verify (part of the open UnZip deviation).
- Every row `check` marks `blocked` or `held`, and every book row held by the
  set rule. Their current files are still upstream, but this tool does not
  baseline them: `check` plans entries at the target version and
  `--include-current` stages only `current` rows. A later run that clears
  the block or hold gives them records.
- Any source whose keyring the operator has not pinned (exit 4).

The first two classes are listed by exact mirror key in
`registry.json` `provenance_exemptions`, each with its reason; the provenance
tests skip those keys and print the exempt count. An exemption ends with its
source: when a bump moves the key or `provenance` writes a record for it,
`test_provenance_exemptions_are_current` fails until the entry is removed.
Never add an exemption for the other two classes; `test_provenance_manifest_complete`
lists them as missing records. Report that list; never write a record that
was not staged.

## Book tracking

`ledger.json` holds the tracked LFS and BLFS versions. `check` compares the
LFS one with the stable book. A book move updates package versions and also
every command, argument, and step order the stage scripts take from the book.
The book map holds that link: each stage block names the book sections it
implements, `script/source/book/` holds those sections' reviewed commands,
and a hash per block and per section shows what changed since the last
review. Stage blocks are the `##`/`###` sections of each stage script's
`formatdoc!` body, plus one block `linux_vorpal: (steps)`: the step sequence
in `sdk/rust/src/artifact/linux_vorpal.rs` (`step_environments`,
`bwrap_arguments`, the stage order, and the inline `rm -rf
$VORPAL_OUTPUT/tools`), mapped to the chapter 7 kernfs, chroot, and cleanup
sections. `book-diff` extracts the text of each section page's
`pre.userinput` and `pre.root` blocks.

When the stable book moves:

1. `script/source.sh book-diff` (default target: stable). It prints, for each
   mapped section whose commands changed, the diff and the current text of
   the stage blocks that implement it. It also prints the commands of each
   chapter 4 to 7 section new in the target book (`unmapped`), and lists
   mapped sections the target dropped (`removed`) and sections that moved in
   the book's order (`moved`). Chapter 8 is skipped by prefix: its new and
   dropped sections appear only as `info:` lines. A section skipped by id is
   snapshotted too, and a change to it is listed. It writes
   `.source-stage/book/plan.json` and staged snapshots. This feeds step 2.
2. After approval, record each new section: `book-diff --skip SECTION
   --reason R`, or `book-diff --map 'FILE:HEADING=SECTION[,SECTION]'` for the
   stage block that will implement it (add the block first; a block mapped
   without a mark keeps no hash, so its section's review in step 4 marks it
   `edited`). For a removed section, remap its blocks with `--map`. A
   section mapped or
   skipped that the plan did not list needs `book-diff` again, since the plan
   snapshots only sections it saw.
3. `script/source.sh book-diff --accept`. It installs the snapshots, moves
   `lfs` and `blfs` `tracked`, `book`, and `patches` to the target, and marks
   each changed or moved section `unreconciled`. On the first move every
   section still `unaudited` becomes `unreconciled` as well, so that move
   reviews every mapped section against its stage blocks, not only the
   changed ones. Check the copyright years in `script/source/book/LICENSE`
   against the new book.
4. For each `unreconciled` section, `book-diff --show SECTION` prints its
   snapshot beside its stage blocks. Compare them command by command: each
   command, argument, and their order, and the order of the blocks against
   the order of the sections. Apply every difference to the stage block,
   adapted to Vorpal paths (`$LFS` is `$VORPAL_OUTPUT` in chapters 5 and 6
   and `/` in chapters 7 and 8; `$LFS_TGT` is `$VORPAL_TARGET`; sources are
   under `$VORPAL_SOURCE`), or record why not. For kernfs, chroot, and
   cleanup, compare with the steps block and with the `step::bwrap` defaults
   in `sdk/rust/src/artifact/step.rs` (`--clearenv`, `/dev`, `/proc`, `/tmp`),
   which no block hashes: Vorpal runs each stage in bubblewrap rather than
   mounting and entering a chroot. Any edit to the steps block needs the same
   edit in `sdk/codegen/src/main.rs`, which hardcodes the step sequence
   (`parse_orchestration_file`), `PATH=/usr/bin:/usr/sbin`, the bwrap
   arguments (`generate_go_bwrap_arguments`, `generate_ts_bwrap_arguments`,
   `generate_py_bwrap_arguments`), and the inline `rm -rf
   $VORPAL_OUTPUT/tools`; `codegen -- --check` does not detect that drift.
   `lfs:chapter07/cleanup` changes on every move (its tar name carries the
   book version), so `unchanged` there needs a reason. Then mark it:
   - `book-diff --mark SECTION --disposition edited` when a stage block
     changed. It refuses when no block of the section changed.
   - `--disposition unchanged` when the blocks already match the book. It
     refuses when a block changed, and needs `--reason` when the book changed
     the section, saying why no block needed an edit.
   - `--disposition deviation --reason R` when Vorpal deliberately differs.
     Also add or update the ledger deviation for a lasting difference.
   Marking refreshes the hashes of the section's blocks. A block shared by
   several sections is refreshed by the first mark, so review every section
   of a shared block before marking any of them. A new or renamed block
   needs `--map` (it combines with `--mark`; a renamed heading changes the
   block text, so it is `edited`). A deleted block is recorded by marking a
   section it mapped to (`edited`), or `--mark local:FILE:HEADING` for a
   local block; a renamed local block takes that mark of its old heading
   with a `--map` of the new one. Blocks moved within or between scripts
   need `--reorder` on the mark. No check compares block order with book
   order; that comparison is this review. `book-diff --unmap SECTION` drops
   a section no block implements any more. Re-confirm each changed skipped
   section with `--skip` and a current reason, or map it. Marking a section
   outside a move keeps an `unaudited` section `unaudited`; only the move's
   review audits it. On the first move, `unchanged` needs no reason for a
   section the book did not change, so that audit is an attestation.
5. Diff the Needed Patches pages (`lfs.patches`, `blfs.patches`). Add,
   rename, or drop patch sources to match.
6. `script/source.sh book-diff --check` and
   `python3 -I sdk/python/tests/test_book_map.py` must pass. List every
   section's disposition, and every deviation reason, in the report.

Outside a book move the same rule holds: any edit to a stage script needs a
`book-diff --mark` (or `--mark local:FILE:HEADING` for a local block), or
`test_book_map.py` and `book-diff --check` fail. An online `--check` that
reports a tracked page no longer matching its snapshot means book errata or
an extractor change: run `book-diff --to <tracked>`, then `--accept`, and
reconcile the sections it marks.

Limits: only `pre.userinput` and `pre.root` text is tracked. Prose, notes,
BLFS dependency lists, and test-suite expectations are not; read the page for
each changed section. A `held` or `blocked` book source does not stop the
book move itself: its section's commands may target a version the SDK does
not yet pin, so say so in that section's reason. `--accept` moves the
tracked versions before `set-version` and before any build. Untracked: text
after a `formatdoc!` body (its arguments), a new stage file until it is added
to `LV_FILES` in `script/source.sh` and `_FILES` in `test_book_map.py`, LFS
chapters 1 to 3 and 9 onward (`stage_03: Setup resolv.conf` implements a
chapter 9 step), and BLFS reorganization: a BLFS page that moves shows only
as `removed`. A `--map FILE:HEADING=local:R` takes the block's hash at once,
so moving an edited block to local records the edit with only that reason;
review the ledger diff for such maps. A deleted block shared by several
sections is acknowledged by marking any one of them; review the others too.
Nothing runs these checks
automatically: `make test` runs only `cargo test`, and no CI job runs
`book-diff --check` or `test_book_map.py`, so the run must.

Mirror keys keep the registry's archive format (GNU `.tar.gz` where the book
lists `.tar.xz`); the GNU signature over that file is the authority, so the
book MD5 does not apply to it.

UnZip is not in BLFS 13.1. The ledger deviation for it is open: `check`
reports unzip as `current` (its track is `pinned`), and the operator chooses
between `bsdunzip` and a recorded exception before linux-vorpal moves past
it. Step 2 asks that question.
