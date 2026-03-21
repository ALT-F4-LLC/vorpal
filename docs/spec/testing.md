---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Documents the current state of testing infrastructure, test coverage, CI test pipelines, and identified gaps across all language SDKs and the Rust core"
owner: "@staff-engineer"
dependencies:
  - code-quality.md
---

# Testing

## Overview

Vorpal has a nascent but growing testing practice. Tests exist in targeted areas (Go SDK config parsing, Rust registry caching) but the overall test surface is thin relative to the codebase size. There is no code coverage tooling, no dedicated test framework beyond the standard library runners for each language, and no integration or end-to-end test harness outside of CI workflow-level build validation.

## Test Inventory

### Rust (Core CLI and SDK)

**Test runner:** `cargo test` via `make test` target in the root `makefile`.

**Existing tests:**

| Location | Type | What It Tests | Test Count |
|---|---|---|---|
| `cli/src/command/start/registry.rs` | Unit (in-module `#[cfg(test)]`) | `ArchiveServer` caching behavior: cache hits, misses across different keys/namespaces, negative caching (not-found), TTL=0 disabling cache, TTL expiration, input validation (empty digest) | 7 |

**Dev dependencies:** `tempfile` (in `cli/Cargo.toml`). No test-specific assertion libraries, property testing frameworks, or mocking crates are used.

**Mocking patterns:** The registry tests use a hand-rolled `MockBackend` implementing the `ArchiveBackend` trait with `Arc<AtomicUsize>` call counters and configurable return values. This is the only mock in the codebase.

**Gaps:**
- No tests for the CLI command layer (`cli/src/command/`)
- No tests for the `config` crate
- No tests for the Rust SDK (`sdk/rust/`)
- No tests for artifact building, worker orchestration, or gRPC service implementations beyond the archive cache
- No integration tests (the `tests/` directory pattern is not used)

### Go SDK

**Test runner:** Standard `go test` (no explicit CI step runs Go tests; they are not invoked by `make test` or the CI workflow).

**Existing tests:**

| File | Type | What It Tests | Test Count |
|---|---|---|---|
| `sdk/go/pkg/config/context_test.go` | Unit | `parseArtifactAlias` — parsing artifact alias strings into name/namespace/tag components, including edge cases (empty, too long, invalid characters, multiple registries, semver tags) | 2 test functions (~30+ table-driven subtests) |
| `sdk/go/pkg/config/context_auth_test.go` | Unit | `ClientAuthHeader` — credential file loading, missing file, valid credentials, registry-not-found, issuer-not-found, invalid JSON, multiple registries; also `GetKeyCredentialsPath` path helpers | 7 |

**Mocking patterns:** Uses function-variable replacement (`getKeyCredentialsPathFunc`) with `defer` cleanup for path mocking. Temp directories via `t.TempDir()`. Standard library `testing` package only — no testify or gomock.

**Gaps:**
- Go SDK tests are not executed in CI (no `go test` step in `.github/workflows/vorpal.yaml`)
- No tests for Go SDK gRPC client code, artifact building, or config loading beyond alias parsing and auth headers
- No tests for the Go SDK template at `cli/src/command/template/go/`

### TypeScript SDK

**Test runner:** `bun test` (configured in `sdk/typescript/package.json` under the `test` script). Dev dependencies include `@types/bun`.

**Existing tests:** None found. No `*.test.ts`, `*.spec.ts`, or `__tests__/` directories exist.

**Gaps:**
- No unit tests despite the `test` script being defined
- No tests for TypeScript SDK artifact building, gRPC client code, or proto serialization
- The `validate:proto` script (`npx tsx scripts/validate-serialization.ts`) exists but is a validation script, not a test suite

## CI Test Pipeline

The GitHub Actions workflow (`.github/workflows/vorpal.yaml`) has a multi-stage pipeline:

```
vendor -> code-quality -> build -> test -> release
```

### Stage: `vendor`
- Runs `make check` (`cargo check`) across a 4-runner matrix (macOS x86/ARM, Ubuntu x86/ARM)
- Validates compilation, not behavior

### Stage: `code-quality`
- Runs `make format` (`cargo fmt --all --check`) — formatting verification
- Runs `make lint` (`cargo clippy -- --deny warnings`) — static analysis
- Single runner (macOS-latest only)

### Stage: `build`
- Runs `make test` (`cargo test`) — executes the 7 Rust unit tests
- Runs `make build` and `make dist`
- Verifies no non-system dynamic library dependencies (custom checks for both macOS `otool` and Linux `ldd`)
- 4-runner matrix

### Stage: `test` (Integration/E2E)
- Downloads the built binary from the `build` stage
- Uses `ALT-F4-LLC/setup-vorpal-action@main` to set up a full Vorpal environment with S3-backed registry
- Builds several artifacts using the Rust SDK config (`Vorpal.toml`): `vorpal`, `vorpal-container-image` (Linux only), `vorpal-job`, `vorpal-process`, `vorpal-shell`, `vorpal-user`
- Cross-validates that Go SDK (`Vorpal.go.toml`) and TypeScript SDK (`Vorpal.ts.toml`) produce identical artifact hashes to the Rust build
- This is the closest thing to end-to-end testing — it validates that all three SDK implementations produce deterministic, matching outputs
- 4-runner matrix

### Nightly Workflow
- `.github/workflows/vorpal-nightly.yaml` — creates nightly releases from `main`, does not run additional tests

## Test Pyramid Assessment

```
         /  E2E  \        CI "test" stage (artifact build + cross-SDK hash matching)
        /----------\
       / Integration \     NONE (no service-level integration tests)
      /----------------\
     /    Unit Tests     \  ~37 tests total (7 Rust, ~30 Go — Go not in CI)
    /____________________\
```

The pyramid is inverted in practice. The most substantial testing happens at the E2E level in CI (full artifact builds across all SDKs), while unit test coverage is extremely sparse. Integration tests do not exist.

## Coverage Tooling

**None.** No coverage tools are configured:
- No `cargo-tarpaulin`, `llvm-cov`, or `grcov` for Rust
- No `-coverprofile` flag usage for Go
- No `c8` or `istanbul` for TypeScript
- No Codecov, Coveralls, or similar reporting integrations

## Test Utilities and Fixtures

- **Rust:** `tempfile` crate available as dev-dependency. No shared test utilities or fixture files.
- **Go:** Standard `t.TempDir()` for filesystem fixtures. Function-variable mocking pattern for dependency injection. No shared test helpers.
- **TypeScript:** No test utilities, fixtures, or helpers.

## Manual Testing Scripts

- `script/test/keycloak.sh` — Interactive script for testing OAuth2 device authorization flow with Keycloak. Tests token exchange between worker/artifact/archive services. Requires running Keycloak instance and manual browser interaction. Not automated.

## Key Observations

1. **Go SDK tests are orphaned from CI** — Tests exist and are well-written (table-driven, good edge case coverage) but are never executed in the CI pipeline.

2. **Cross-SDK determinism is the primary quality gate** — The CI `test` stage validates that Rust, Go, and TypeScript SDKs produce byte-identical artifacts. This is a strong correctness signal for the build system but does not test error paths, edge cases, or failure modes.

3. **No test for the CLI user experience** — The CLI command layer has zero tests. All `cli/src/command/` modules are untested.

4. **Static analysis compensates partially** — `cargo clippy --deny warnings` and `cargo fmt --check` catch categories of bugs and style issues at compile time, partially offsetting the thin unit test coverage for the Rust codebase.

5. **No test isolation for gRPC services** — The gRPC service implementations (agent, artifact, archive, context, worker) have no unit or integration tests. The only service-level test is the archive cache test.

6. **Auth flow testing is manual only** — The `keycloak.sh` script validates the OAuth2 token exchange flow but requires manual interaction and a running Keycloak instance. No automated auth testing exists.
