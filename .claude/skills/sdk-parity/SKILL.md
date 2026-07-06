---
name: sdk-parity
description: "Test parity between Rust, Go, and TypeScript SDK builds by comparing artifact digests"
---

# SDK Parity Testing

Test consistency between Rust, Go, and TypeScript SDK builds by comparing artifact digests.

## Usage

```
/sdk-parity [artifact-name]
```

**Default artifact:** `vorpal`

## Instructions

When this skill is invoked:

### 1. Parse Arguments

Extract the artifact name from the arguments. If no argument is provided, use `vorpal` as the default.

### 2. Validate Artifact Name

The artifact must be one of the following known artifacts:

- `vorpal-container-image` (linux only)
- `vorpal-job`
- `vorpal-process`
- `vorpal-shell`
- `vorpal-user`
- `vorpal-website`
- `vorpal`

If the artifact name is not in this list, report an error and list the valid options.

### 3. Prepare Lima VM (Linux-Only Artifacts)

Classify the artifact:

- **Linux-only:** `vorpal-container-image`
- **All platforms:** all other artifacts

**If the artifact is linux-only**, run the following steps before starting services:

1. Check if the `vorpal-aarch64` Lima instance is running:

```bash
limactl list --format '{{.Name}}:{{.Status}}' | grep 'vorpal-aarch64'
```

2. If the instance is not running, start it:

```bash
make lima
```

3. Sync the project to the Lima VM:

```bash
make lima-sync
```

**If the artifact is not linux-only**, skip this section entirely.

### 4. Determine Socket Path

The socket path is derived from the makefile variable `VORPAL_SOCKET`. It follows the pattern:

```
/tmp/vorpal-<directory-basename>.sock
```

where `<directory-basename>` is the basename of the current working directory (e.g., `/tmp/vorpal-sdk-parity.sock`). Run `make -n -p | grep '^VORPAL_SOCKET'` to confirm the exact value if needed.

**For linux-only artifacts:** The socket path follows the same pattern but exists inside the Lima VM. Commands that check or manipulate the socket must run via `limactl shell vorpal-aarch64`.

### 5. Stop Any Existing Services

Remove any existing socket file and kill associated processes to ensure a clean state.

**For linux-only artifacts:**

```bash
limactl shell vorpal-aarch64 bash -c 'VORPAL_SOCK="/tmp/vorpal-vorpal.sock"; if [ -e "$VORPAL_SOCK" ]; then fuser -k "$VORPAL_SOCK" 2>/dev/null || true; rm -f "$VORPAL_SOCK"; fi'
```

**For non-linux artifacts:**

```bash
VORPAL_SOCK="/tmp/vorpal-$(basename "$PWD").sock"
if [ -e "$VORPAL_SOCK" ]; then fuser -k "$VORPAL_SOCK" 2>/dev/null || true; rm -f "$VORPAL_SOCK"; fi
```

### 6. Start Services

**For linux-only artifacts:**

Start Vorpal services inside the Lima VM in the background using `run_in_background: true`:

```bash
limactl shell vorpal-aarch64 bash -c "cd ~/vorpal && target/debug/vorpal system services start"
```

Then wait for the socket file to appear inside the VM (up to 60 seconds):

```bash
VORPAL_SOCK="/tmp/vorpal-vorpal.sock"
for i in {1..60}; do limactl shell vorpal-aarch64 bash -c "[ -S \"$VORPAL_SOCK\" ]" && echo "Services ready after ${i}s" && break; [ $i -eq 60 ] && echo "ERROR: Services failed to start (socket not found inside Lima VM: $VORPAL_SOCK)" && exit 1; sleep 1; done
```

**For non-linux artifacts:**

Start Vorpal services in the background using `run_in_background: true`:

```bash
make vorpal-start
```

Then wait for the socket file to appear (up to 60 seconds):

```bash
VORPAL_SOCK="/tmp/vorpal-$(basename "$PWD").sock"
for i in {1..60}; do [ -S "$VORPAL_SOCK" ] && echo "Services ready after ${i}s" && break; [ $i -eq 60 ] && echo "ERROR: Services failed to start (socket not found: $VORPAL_SOCK)" && exit 1; sleep 1; done
```

If services fail to start, report the error and stop.

### 7. Run Rust SDK Build

**For linux-only artifacts:**

```bash
limactl shell vorpal-aarch64 bash -c "cd ~/vorpal && target/debug/vorpal build <artifact-name>"
```

**For non-linux artifacts:**

```bash
make VORPAL_ARTIFACT="<artifact-name>" vorpal
```

Capture the output and extract the digest from the build result. The digest appears in the output as a hash value.

### 8. Run Go SDK Build

**For linux-only artifacts:**

```bash
limactl shell vorpal-aarch64 bash -c "cd ~/vorpal && target/debug/vorpal build --config 'Vorpal.go.toml' <artifact-name>"
```

**For non-linux artifacts:**

```bash
make VORPAL_ARTIFACT="<artifact-name>" VORPAL_FLAGS="--config 'Vorpal.go.toml'" vorpal
```

Capture the output and extract the digest from the build result.

### 9. Run TypeScript SDK Build

**For linux-only artifacts:**

```bash
limactl shell vorpal-aarch64 bash -c "cd ~/vorpal && target/debug/vorpal build --config 'Vorpal.ts.toml' <artifact-name>"
```

**For non-linux artifacts:**

```bash
make VORPAL_ARTIFACT="<artifact-name>" VORPAL_FLAGS="--config 'Vorpal.ts.toml'" vorpal
```

Capture the output and extract the digest from the build result.

### 10. Stop Services

Always stop services after builds complete (whether successful or not).

**For linux-only artifacts:**

```bash
limactl shell vorpal-aarch64 bash -c 'VORPAL_SOCK="/tmp/vorpal-vorpal.sock"; if [ -e "$VORPAL_SOCK" ]; then fuser -k "$VORPAL_SOCK" 2>/dev/null || true; rm -f "$VORPAL_SOCK"; fi'
```

**For non-linux artifacts:**

```bash
VORPAL_SOCK="/tmp/vorpal-$(basename "$PWD").sock"
if [ -e "$VORPAL_SOCK" ]; then fuser -k "$VORPAL_SOCK" 2>/dev/null || true; rm -f "$VORPAL_SOCK"; fi
```

### 11. Compare Digests

Compare all extracted digests (Rust, Go, and TypeScript). All digests must match for the test to pass.

### 12. Report Results

Display a summary table:

```
## SDK Parity Test Results

| SDK        | Digest            |
|------------|-------------------|
| Rust       | <rust-digest>     |
| Go         | <go-digest>       |
| TypeScript | <ts-digest>       |

**Status:** PASS (digests match)
```

Or if they don't match:

```
**Status:** FAIL (digests differ)
```

If an SDK was skipped (e.g., TypeScript for `vorpal-shell`), note it in the table:

```
| TypeScript | skipped (not supported) |
```

## Prerequisites

- `Vorpal.toml`, `Vorpal.go.toml`, and `Vorpal.ts.toml` must exist in the working directory

## Notes

- Services are always started and stopped as part of the skill — never assume they are already running
- Run builds sequentially (Rust first, then Go, then TypeScript) to avoid resource contention
- If any build fails, stop services, report the failure, and do not attempt comparison
- Extract the full digest hash from each build output for accurate comparison
- Lima VM commands use `vorpal-aarch64` which assumes Apple Silicon (aarch64). On x86_64 hosts, the VM name would be `vorpal-x86_64` — adjust commands accordingly.

## Serializer fixture parity (offline, no services)

The build-based flow above exercises the full pipeline end-to-end. A second,
lightweight arm validates only the digest-parity SERIALIZER against shared
fixtures with NO running services. This is the load-bearing cross-SDK invariant:
every SDK hand-writes a serializer and must produce byte-identical SHA-256
artifact digests for the same artifact definition.

**Shared assets** (additive home; new SDK arms extend, never rewrite):

- `sdk/python/tests/fixtures/digest-parity/artifacts.json` — language-neutral
  artifact fixtures. Optional fields (`digest`, `entrypoint`, `script`) are
  presence-encoded: a key PRESENT (even `""`) means proto-present; a key ABSENT
  means proto-absent (serializes to `null`).
- `sdk/python/tests/fixtures/digest-parity/digests.json` — golden digests
  produced by the canonical TypeScript reference serializer
  (`serializeArtifact`/`computeArtifactDigest` in
  `sdk/typescript/src/context.ts`) over those fixtures.

**Python arm:** `python sdk/python/tests/test_parity.py` builds each fixture as
a proto `Artifact`, serializes via `vorpal_sdk.context`, and asserts the digest
equals the golden. Exits 0 on parity, non-zero (with the produced JSON) on any
mismatch. Also runnable under `pytest`.

**Regenerating goldens** (only after a deliberate, cross-SDK-coordinated format
change): run the TS reference serializer over `artifacts.json` and overwrite the
digest values in `digests.json`. Never edit a golden to match a single SDK in
isolation — that hides a real cross-SDK divergence.

**Coordination:** the builder-output parity arm (DKT-19) extends this section
additively with its own builder-input fixtures; it does not modify the
serializer fixtures or goldens above.

## Cross-SDK BUILDER parity (build-target Python — DKT-19)

The serializer arm above proves the digest *serializer* agrees across SDKs. This
arm proves the *builders* agree: the same minimal Python build-target project,
built through each SDK's Python language builder (`python.rs` / `python.go` /
`python.ts`), produces byte-identical artifact digests **per system**. It is the
build-target analogue of the build-based `vorpal` flow (steps 6-12 above) — same
machinery, a Python artifact instead of `vorpal`. Distinct from:
- the *serializer-fixture* parity arm above (offline, no builders), and
- DKT-16's *sdk/python builder-output* arm (the Python SDK's own builder output),
  and DKT-20's cross-LANGUAGE config parity. This arm is cross-SDK BUILDER parity.

**Fixture project:** the `vorpal init` Python template (`cli/src/command/template/python/`)
is the minimal build-target project — reuse it; do not author a parallel fixture.
Build it once per SDK via the build flow above (steps 6-12), substituting the
Python artifact name for `vorpal`, with `Vorpal.toml` / `Vorpal.go.toml` /
`Vorpal.ts.toml` selecting the Rust/Go/TS builder.

**Invariant:** within one system, Rust == Go == TS digest (PASS). Cross-system
divergence is EXPECTED and NOT a failure (native/compiled-extension deps differ
per platform; TDD §Testing Strategy untested-claims) — assert same-system only.

**The one adaptation beyond fixture + artifact name:** steps 6-12 compare a
single cross-SDK digest per run. Here, run the build/compare PER SYSTEM and group
the comparison by `ArtifactSystem` — for each system, assert the Rust/Go/TS
builder digests are identical; never compare digests across systems. Everything
else (start services, build per-SDK, extract digest) is reused unchanged.

**RUNTIME-GATED.** This arm cannot run until both land:
- the `cpython` / `uv` toolchain is published on `sdk.vorpal.build` (DKT-25 — the
  mirror returns 403 today), and
- the pinned 3.13.14 interpreter + uv 0.10.11 are provisioned in the build env
  (DKT-21; `cpython.rs` / `uv.rs` are intentionally unpinned until capture).
Until then the build fails closed at the C1 mint gate (`unpinned - use --unlock`),
so a build-and-compare run is not yet meaningful. Run this arm as part of the
`vorpal` build-flow validation once DKT-21/DKT-25 close; the version-equality
invariant it depends on is enforced now by
`sdk/python/tests/test_version_equality.py`.
