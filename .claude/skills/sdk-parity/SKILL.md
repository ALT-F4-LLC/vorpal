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
