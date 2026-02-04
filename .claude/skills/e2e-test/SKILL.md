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
3. **Wait for services** to be ready on port 23152
4. **Run artifact build** against the services
5. **Stop all services** on port 23152
6. **Report results**

## Execution Instructions

### Step 1: Build if needed

Check if the debug binary exists. If not, build the project:

```bash
[ -f target/debug/vorpal ] || make build
```

### Step 2: Start services in background

Use the Bash tool with `run_in_background: true` to start services:

```bash
make vorpal-start
```

The service logs will stream to the shell output for review.

### Step 3: Wait for services

Wait for port 23152 to be available (up to 60 seconds):

```bash
for i in {1..60}; do nc -z localhost 23152 2>/dev/null && echo "Services ready after ${i}s" && break; [ $i -eq 60 ] && echo "ERROR: Services failed to start" && exit 1; sleep 1; done
```

### Step 4: Run artifact build

Build the artifact against the running services. Default artifact is `vorpal-shell`:

```bash
make VORPAL_ARTIFACT="vorpal-shell" vorpal
```

To test with a different artifact:

```bash
make VORPAL_ARTIFACT="<artifact-name>" vorpal
```

### Step 5: Stop services

After the build completes (success or failure), stop all services on port 23152:

```bash
lsof -ti:23152 | xargs kill 2>/dev/null || true
```

### Step 6: Report results

- If build succeeded, report "E2E TEST PASSED"
- If build failed, review the service logs in the shell output and the build error messages

## Arguments

The skill accepts an optional artifact name. Default is `vorpal-shell`:

- `/e2e-test` - Test with vorpal-shell artifact
- `/e2e-test vorpal` - Test with vorpal artifact
- `/e2e-test <name>` - Test with specified artifact

## Troubleshooting

- **Port in use**: `lsof -ti:23152 | xargs kill`
- **Build fails**: Run `make build` first
- **Services crash**: Review shell output for error messages
