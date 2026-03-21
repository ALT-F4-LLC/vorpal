---
project: "vorpal"
maturity: "proof-of-concept"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Current state of testing infrastructure, coverage, and gaps across the vorpal project"
owner: "@staff-engineer"
dependencies:
  - code-quality.md
---

# Testing

## Overview

Vorpal has minimal test coverage. The project is a multi-language workspace (Rust CLI/config/SDK, Go SDK, TypeScript SDK) with a small number of unit tests concentrated in two areas. There are no integration test suites, no end-to-end test framework, and no coverage tooling configured. The CI pipeline includes a `cargo test` step and a functional smoke-test job that exercises the built binary across SDKs, but no dedicated test reporting or coverage gates exist.

## Test Inventory

### Rust (CLI / Config / SDK)

**Location:** `cli/src/command/start/registry.rs` (inline `#[cfg(test)]` module)

**What exists:**
- 7 async unit tests for the `ArchiveServer` cache layer using `#[tokio::test]`
- Tests cover: cache hit/miss, namespace isolation, negative caching, TTL disable (zero), TTL expiration, and input validation (empty digest)
- Uses a hand-rolled `MockBackend` implementing the `ArchiveBackend` trait with `Arc<AtomicUsize>` call counters
- No external test dependencies beyond `tokio` (already a runtime dependency)

**Dev dependencies (cli/Cargo.toml):**
- `tempfile = "3.24.0"` — available but not currently used in any test

**What does not exist:**
- No tests in `config/` or `sdk/rust/` crates
- No test utilities crate or shared test helpers
- No property-based testing (e.g., proptest, quickcheck)
- No snapshot testing
- No coverage tooling (cargo-tarpaulin, cargo-llvm-cov, etc.)
- No benchmark tests (criterion or built-in)

### Go SDK

**Location:** `sdk/go/pkg/config/`

**What exists:**
- `context_test.go` — 3 test functions covering `parseArtifactAlias`:
  - `TestParseArtifactAlias`: table-driven test with 30+ cases (basic formats, real-world examples, edge cases, invalid characters, error conditions)
  - `TestParseArtifactAliasDefaults`: 4-case table test for default value application
- `context_auth_test.go` — 7 test functions covering `ClientAuthHeader`:
  - Tests cover: missing file, valid credentials, registry not found, issuer not found, invalid JSON, multiple registries, path helpers
  - Uses `t.TempDir()` for file-system isolation
  - Uses function-variable mocking pattern (`getKeyCredentialsPathFunc` replacement with deferred restore)

**What does not exist:**
- No tests outside the `config` package
- No test for gRPC client/server interactions
- No testify or gomock — uses only stdlib `testing`
- No `go test` step in CI (only `cargo test` is run via `make test`)
- No coverage reporting (`-coverprofile`, etc.)

### TypeScript SDK

**What exists:**
- `package.json` declares `"test": "bun test"` script
- Bun is the configured test runner

**What does not exist:**
- Zero test files (`*.test.ts`, `*.spec.ts`) — the test script exists but has nothing to run
- No test utilities, fixtures, or mocks
- No CI step running TypeScript tests

## CI Testing Pipeline

### Main Workflow (`.github/workflows/vorpal.yaml`)

The pipeline has a four-stage dependency chain: `vendor` -> `code-quality` -> `build` -> `test`.

**Build stage (`build` job):**
- Runs `make TARGET=release test` which executes `cargo test --offline --release`
- Runs on 4 matrix runners: `macos-latest`, `macos-latest-large`, `ubuntu-latest`, `ubuntu-latest-arm64`
- Also includes dynamic-library dependency verification (non-test but quality-related)

**Functional test stage (`test` job):**
- Builds vorpal artifacts using the compiled binary itself (`vorpal build "vorpal"`, `vorpal build "vorpal-job"`, etc.)
- Cross-validates SDK parity: builds the same artifacts using Rust, Go (`Vorpal.go.toml`), and TypeScript (`Vorpal.ts.toml`) configs, then asserts artifact hash equality
- This is the closest thing to an integration/e2e test — it validates that the full build pipeline produces deterministic, SDK-consistent results
- Uses `ALT-F4-LLC/setup-vorpal-action@main` with S3 registry backend
- Runs on the same 4-runner matrix

**What CI does not do:**
- No Go test execution (`go test ./...`)
- No TypeScript test execution (`bun test`)
- No coverage collection or reporting
- No test result artifacts or reporting (JUnit XML, etc.)
- No flaky-test detection or retry logic
- No test-specific caching (only Cargo target/vendor dirs)

### Nightly Workflow (`.github/workflows/vorpal-nightly.yaml`)

- Creates a nightly tag from `main` — no additional tests beyond what the main workflow runs

## Test Pyramid Assessment

| Level | Status | Details |
|-------|--------|---------|
| Unit | Minimal | ~17 tests total (7 Rust, 10 Go), concentrated in 2 packages |
| Integration | None | No inter-component or service-level tests |
| E2E | Partial (CI only) | Functional smoke tests in CI via `vorpal build` cross-SDK validation |
| Performance | None | No benchmarks or load tests |

## Mocking Patterns

Two distinct approaches are used:

1. **Rust — Trait-based mocking:** `MockBackend` manually implements `ArchiveBackend` trait. No mocking framework. Suitable for the current scale but will not scale if more traits need mocking.

2. **Go — Function-variable replacement:** `getKeyCredentialsPathFunc` is a package-level `var` holding a function, swapped in tests with `defer` restore. This is a common Go pattern but couples test setup to implementation details.

Neither language uses a mocking framework (mockall, gomock, testify/mock).

## Gaps and Risks

### Critical Gaps

1. **No Go or TypeScript tests in CI.** Go tests exist but are never run in the pipeline. TypeScript has zero tests.
2. **No coverage measurement.** There is no way to know what percentage of code is tested or whether coverage is trending up or down.
3. **No integration tests.** The gRPC service layer (agent, artifact, archive, context, worker) has no test coverage for client-server interactions.
4. **CLI command coverage is zero.** The CLI entry points, argument parsing, and command dispatch are untested.

### Moderate Gaps

5. **No test for the config crate (Rust).** The `config/` workspace member has no tests.
6. **No test for the Rust SDK crate.** `sdk/rust/` has no tests despite being a published crate (`vorpal-sdk` on crates.io).
7. **Functional CI tests are opaque.** The `test` job validates artifact hash equality but has no structured assertions — failures produce unhelpful shell error messages.
8. **No test isolation for filesystem operations.** The Go auth tests use `t.TempDir()` correctly, but Rust tests do not exercise filesystem paths despite `tempfile` being available.

### Low Priority

9. **No property-based or fuzz testing** for parser code (`parseArtifactAlias`, config deserialization).
10. **No snapshot testing** for generated protobuf code or API responses.

## Test Execution

### Local

```bash
# Rust unit tests (all workspace crates)
make test                    # debug mode
make TARGET=release test     # release mode (matches CI)

# Go unit tests (not wired into make)
cd sdk/go && go test ./...

# TypeScript (no tests to run)
cd sdk/typescript && bun test
```

### CI

Tests are triggered on all pull requests and pushes to `main`. The `test` job requires the `build` job to complete first and uses uploaded dist artifacts rather than rebuilding.
