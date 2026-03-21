---
project: "vorpal"
maturity: experimental
last_updated: "2026-03-20"
updated_by: "@staff-engineer"
scope: "Test infrastructure, coverage, test types, and testing gaps"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - code-quality.md
---

# Testing

## Overview

Vorpal's test coverage is minimal at the unit level but has a strong integration/end-to-end testing strategy via CI. The primary quality gate is the cross-SDK parity test, which verifies that Rust, Go, and TypeScript SDKs produce identical artifact digests. Unit tests exist only in targeted areas.

## Test Infrastructure

### Test Runners

| Language | Runner | Command | CI Integration |
|----------|--------|---------|----------------|
| Rust | `cargo test` | `make test` | Yes (build job) |
| Go | `go test` | Not in makefile | Not in CI |
| TypeScript | `bun test` | `npm run test` | Not in CI |

### Test Dependencies

- **Rust**: `tempfile` (dev-dependency in `cli/Cargo.toml`) for temporary file handling in tests
- **Go**: Standard `testing` package
- **TypeScript**: Bun's built-in test runner

## Existing Tests

### Rust Unit Tests

**`cli/src/command/start/registry.rs`** -- Archive service cache tests (7 tests):

1. `test_cache_hit_skips_backend` -- Verifies that cached archive checks skip the storage backend
2. `test_cache_miss_for_different_keys` -- Different digest keys result in separate cache entries
3. `test_cache_miss_for_different_namespaces` -- Namespace isolation in the cache
4. `test_negative_caching_not_found` -- "Not found" responses are cached to avoid redundant lookups
5. `test_ttl_zero_disables_caching` -- TTL=0 bypasses the cache entirely
6. `test_ttl_expiration` -- Entries expire after the configured TTL
7. `test_check_returns_error_for_empty_digest` -- Input validation

These tests use a `MockBackend` with an `AtomicUsize` call counter to verify caching behavior without hitting real storage.

This is the **only** Rust test module in the codebase (outside of `target/` build artifacts).

### Go Tests

**`sdk/go/pkg/config/context_test.go`** (365 lines):
- Tests for the Go SDK's configuration context and build orchestration

**`sdk/go/pkg/config/context_auth_test.go`** (288 lines):
- Tests for the Go SDK's authentication context and credential handling

These tests are **not** run in CI -- there is no `go test` step in any workflow.

### TypeScript Tests

`bun test` is configured in `package.json` but no test files were found in `sdk/typescript/`. The test command exists but has no tests to run.

## Integration / End-to-End Tests

### Cross-SDK Parity (CI `test` job)

The most critical test in the project. After building the Vorpal binary, CI:

1. Builds 6 artifact types using the Rust SDK config (`Vorpal.toml`)
2. Rebuilds the same artifacts using Go SDK config (`Vorpal.go.toml`)
3. Rebuilds again using TypeScript SDK config (`Vorpal.ts.toml`)
4. Compares digests: Go and TypeScript must match Rust for every artifact

This runs on all 4 platform runners (macOS ARM64, macOS x86_64, Ubuntu ARM64, Ubuntu x86_64).

Artifacts tested:
- `vorpal` -- The CLI binary itself
- `vorpal-container-image` -- Container image (Linux only)
- `vorpal-job` -- Job artifact
- `vorpal-process` -- Process artifact
- `vorpal-shell` -- Shell environment
- `vorpal-user` -- User environment

### Self-Hosted Build (CI `test` job)

Vorpal builds itself using its own build system. This serves as both an integration test and a dogfooding exercise. The `setup-vorpal-action` GitHub Action installs and configures Vorpal with S3 registry backend.

### Dynamic Library Verification (CI `build` job)

Platform-specific checks ensure the binary has no non-system dynamic library dependencies:
- macOS: `otool -L` checks for homebrew/local lib paths
- Linux: `ldd` checks for non-system library paths

### Keycloak Integration Test

`script/test/keycloak.sh` exists for testing the OIDC/Keycloak integration but is not run in CI.

## Test Pyramid Assessment

```
         /‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾\
        /   E2E / Parity    \     <-- Strong (cross-SDK, 4 platforms)
       /     (CI test job)    \
      /‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾\
     /      Integration         \  <-- Moderate (self-hosted build)
    /     (self-build, services) \
   /‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾\
  /         Unit Tests             \ <-- Weak (7 Rust tests, ~650 lines Go)
 /     (cache, config, auth)        \
/‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾\
```

The pyramid is inverted: strong at the top (E2E) but weak at the base (unit tests).

## Coverage

No code coverage tools are configured:
- No `cargo tarpaulin`, `cargo llvm-cov`, or `grcov`
- No `go test -cover`
- No Istanbul/c8 for TypeScript
- No coverage thresholds or badges
- No coverage reporting in CI

## Mocking and Test Utilities

- **Rust**: `MockBackend` struct in `registry.rs` -- manual mock implementation with `Arc<AtomicUsize>` for call counting and `Arc<Mutex<HashMap>>` for state
- **Go**: Standard Go test patterns (no external mocking framework observed)
- **No shared test fixtures or factories across the codebase**

## Gaps and Areas for Improvement

### Critical Gaps

- **No unit tests for digest computation** -- The most correctness-critical code path (`hashes.rs`) has no tests
- **No unit tests for agent/source resolution** -- Complex branching logic with HTTP, local, and git source types
- **No unit tests for auth validation** -- OIDC validation, namespace permissions untested at unit level
- **No unit tests for config resolution** -- Layered configuration merging untested
- **Go tests not in CI** -- 653 lines of tests exist but are never run in the pipeline
- **No TypeScript tests** -- Test runner configured but no test files

### Recommended Additions

1. Unit tests for `get_source_digest` and file hashing determinism
2. Unit tests for `ArtifactStepSecret` encryption/decryption roundtrip
3. Unit tests for layered config resolution (user + project + CLI)
4. Unit tests for lockfile serialization/deserialization
5. Integration test for UDS socket lifecycle (bind, stale detection, cleanup)
6. Add `go test ./...` step to CI workflow
7. Add coverage tooling and reporting
8. Property-based tests for proto serialization determinism across SDKs
