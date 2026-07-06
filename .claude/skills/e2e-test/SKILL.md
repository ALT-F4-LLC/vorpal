---
name: e2e-test
description: Run end-to-end tests that validate Vorpal services and client builds. Use when testing the full system (services + build pipeline), validating changes work end-to-end, or verifying services and clients communicate correctly.
allowed-tools: Bash, Read, Glob, Grep
---

# End-to-End Test Skill

Run end-to-end tests to validate that Vorpal services start correctly and can successfully build artifacts.

## Test Flow

1. **Build the project** (if not already built)
2. **Start backend services** in background using `run_in_background: true`
3. **Wait for services** to be ready on port 23153
4. **Run artifact build** against the services
5. **Stop all services** on port 23153
6. **Report results**

## Execution Instructions

### Step 1: Start services in background

Use the Bash tool with `run_in_background: true` to start services:

```bash
make vorpal-start
```

The service logs will stream to the shell output for review.

### Step 2: Wait for services

Wait for port 23153 to be available (up to 60 seconds):

```bash
for i in {1..60}; do nc -z localhost 23153 2>/dev/null && echo "Services ready after ${i}s" && break; [ $i -eq 60 ] && echo "ERROR: Services failed to start" && exit 1; sleep 1; done
```

### Step 3: Run artifact build

Build the artifact against the running services. Default artifact is `vorpal-shell`:

```bash
make VORPAL_ARTIFACT="vorpal-shell" vorpal-build
```

To test with a different artifact:

```bash
make VORPAL_ARTIFACT="<artifact-name>" vorpal-build
```

### Step 4: Stop services

After the build completes (success or failure), stop all services on port 23153:

```bash
lsof -ti:23153 | xargs kill 2>/dev/null || true
```

### Step 5: Report results

- If build succeeded, report "E2E TEST PASSED"
- If build failed, review the service logs in the shell output and the build error messages

## Arguments

The skill accepts an optional artifact name. Default is `vorpal`:

- `/e2e-test` - Test with vorpal-shell artifact
- `/e2e-test vorpal` - Test with vorpal artifact
- `/e2e-test <name>` - Test with specified artifact

## `vorpal prepare` integration ACs (DKT-71)

These cover the integration-level `vorpal prepare` acceptance criteria that
cannot be unit-tested in isolation because they require a live
agent/worker/registry (real gRPC round-trip), not a Rust integration harness.
They extend — do not replace — the build flow above. Run each against live
services started via `make vorpal-start` (Step 1) and stop them after (Step 4).

> Makefile targets were split: `vorpal-build` (was `vorpal`) runs `vorpal build`,
> `vorpal-prepare` runs `vorpal prepare`.

### AC (a): `prepare` never invokes the worker build for the target graph

**Runnable locally** (single host + services).

```bash
make vorpal-start &
# wait for port 23153 (see Step 2)
make VORPAL_ARTIFACT="<artifact-name>" vorpal-prepare
```

Expected: the command resolves/pins sources and prints the mint/update/verify
summary plus the resolved digest, and the output contains **no** build-artifact
RPC or worker build invocation (no `building artifact ...` / no worker step
output). `prepare` mints and verifies; it does not build.

### AC (b): two-host remote-tarball fetch produces a byte-identical lock entry

**CI / manual only** — requires two host arch/OS combinations (e.g. an
aarch64-darwin host and an x86_64-linux runner) preparing the same source.

On each host (against that host's services):

```bash
make VORPAL_ARTIFACT="<artifact-name>" vorpal-prepare
```

Then diff the resulting `Vorpal.lock` source entry across the two hosts. The
pin (digest) MUST be byte-identical. (Cross-host determinism assumption:
ADR 0002 — normalization-stable filenames within a source.)

### AC (c): `--unlock=false` enforces fail-closed end-to-end

**Runnable locally** if an unpinned remote source is present; otherwise CI.

```bash
make vorpal-start &
# wait for port 23153
VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo run --bin vorpal -- prepare \
    --config <config> --unlock=false <artifact>
```

Expected against an **unpinned** remote source: the command fails closed with
`source '...' is unpinned - use --unlock to pin` (the agent gate fires through
the full prepare path, not just the isolated clap parse).

### AC (d): `prepare` (default unlock) and `build --unlock` produce identical lock entries

**Runnable locally** (single host + services).

```bash
make vorpal-start &
# wait for port 23153
# snapshot the lock after prepare (default --unlock=true)
cp Vorpal.lock /tmp/lock.prepare
make VORPAL_ARTIFACT="<artifact-name>" vorpal-prepare
cp Vorpal.lock /tmp/lock.prepare

# then build with explicit --unlock and snapshot again
VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo run --bin vorpal -- build \
    --config <config> --unlock <artifact>
cp Vorpal.lock /tmp/lock.build

diff /tmp/lock.prepare /tmp/lock.build
```

Expected: the diff is empty — `prepare` (default `--unlock true`) and
`build --unlock` write byte-identical `Vorpal.lock` entries for the same target.

## Troubleshooting

- **Port in use**: `lsof -ti:23153 | xargs kill`
- **Build fails**: Run `make build` first
- **Services crash**: Review shell output for error messages
