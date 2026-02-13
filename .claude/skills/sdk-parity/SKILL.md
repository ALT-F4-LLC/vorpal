---
name: sdk-parity
description: "Test parity between Rust and Go SDK builds by comparing artifact digests"
---

# SDK Parity Testing

Test consistency between Rust and Go SDK builds by comparing artifact digests.

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

- `vorpal`
- `vorpal-container-image`
- `vorpal-job`
- `vorpal-process`
- `vorpal-release`
- `vorpal-shell`
- `vorpal-user`

If the artifact name is not in this list, report an error and list the valid options.

### 3. Run Rust SDK Build

Execute:

```bash
make VORPAL_ARTIFACT="<artifact-name>" vorpal
```

Capture the output and extract the digest from the build result. The digest appears in the output as a hash value.

### 4. Run Go SDK Build

Execute:

```bash
make VORPAL_ARTIFACT="<artifact-name>" VORPAL_FLAGS="--config 'Vorpal.go.toml'" vorpal
```

Capture the output and extract the digest from the build result.

### 5. Compare Digests

Compare the two extracted digests.

### 6. Report Results

Display a summary table:

```
## SDK Parity Test Results

| SDK  | Digest |
|------|--------|
| Rust | <rust-digest> |
| Go   | <go-digest> |

**Status:** PASS ✓ (digests match)
```

Or if they don't match:

```
**Status:** FAIL ✗ (digests differ)
```

## Prerequisites

- Vorpal services must be running (`make vorpal-start`)
- Both `Vorpal.toml` and `Vorpal.go.toml` must exist in the working directory

## Notes

- Run builds sequentially (Rust first, then Go) to avoid resource contention
- If either build fails, report the failure and do not attempt comparison
- Extract the full digest hash from each build output for accurate comparison
