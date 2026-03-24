---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "Test strategy, coverage, infrastructure, and gaps across all language SDKs and CLI"
owner: "@staff-engineer"
dependencies:
  - code-quality.md
---

# Testing

## Overview

Vorpal's testing footprint is minimal relative to codebase size. Tests exist in two locations: the Go SDK (`sdk/go/`) and a single Rust module in the CLI (`cli/src/command/start/registry.rs`). There are no tests in the Rust SDK (`sdk/rust/`), no integration test suites, and no dedicated test infrastructure beyond what CI provides. The project relies heavily on end-to-end build verification in CI rather than unit or integration tests.

## Test Inventory

### Rust (CLI)

**Location:** `cli/src/command/start/registry.rs` (lines 504-713)

A single `#[cfg(test)]` module containing 7 async tests for the `ArchiveServer` caching layer:

| Test | What it verifies |
|------|------------------|
| `test_cache_hit_skips_backend` | Second check for same key hits cache, backend called once |
| `test_cache_miss_for_different_keys` | Different digests produce separate cache entries |
| `test_cache_miss_for_different_namespaces` | Same digest in different namespaces are distinct cache keys |
| `test_negative_caching_not_found` | NotFound results are cached (negative caching) |
| `test_ttl_zero_disables_caching` | TTL=0 disables caching entirely |
| `test_ttl_expiration` | Cache entries expire after TTL |
| `test_check_returns_error_for_empty_digest` | Empty digest returns InvalidArgument without hitting backend |

**Patterns used:**
- `#[tokio::test]` for async test execution
- Mock `ArchiveBackend` trait implementation with `Arc<AtomicUsize>` call counters
- Given/When/Then commenting convention
- No external test utilities or frameworks beyond `tokio`

**Dev dependencies:** `tempfile = "3.24.0"` (declared in `cli/Cargo.toml` but not used in current tests)

### Rust (SDK)

**No tests exist.** `sdk/rust/Cargo.toml` has no `[dev-dependencies]` section and no test modules were found in the crate.

### Go SDK

**Location:** `sdk/go/pkg/config/`

Two test files with standard `testing` package:

1. **`context_test.go`** — Tests for `parseArtifactAlias()` function:
   - `TestParseArtifactAlias`: 25+ table-driven test cases covering basic formats, real-world examples, edge cases (multiple colons, special characters), semantic versions, numeric components, and error conditions (empty string, empty tag, too many slashes, invalid characters, length limits)
   - `TestParseArtifactAliasDefaults`: 4 cases verifying default tag ("latest") and namespace ("library") application

2. **`context_auth_test.go`** — Tests for `ClientAuthHeader()` function:
   - `TestClientAuthHeaderNoFile`: Missing credentials file returns empty string
   - `TestClientAuthHeaderValid`: Valid credentials produce correct Bearer token
   - `TestClientAuthHeaderRegistryNotFound`: Unknown registry returns empty string
   - `TestClientAuthHeaderIssuerNotFound`: Missing issuer returns error
   - `TestClientAuthHeaderInvalidJSON`: Malformed JSON returns parse error
   - `TestClientAuthHeaderMultipleRegistries`: Correct token selection across multiple registries
   - `TestGetKeyCredentialsPath`: Path helper functions return expected values

**Patterns used:**
- Table-driven tests (idiomatic Go pattern)
- `t.TempDir()` for filesystem isolation
- Function-variable mocking (`getKeyCredentialsPathFunc` replacement with deferred restore)
- `t.Run()` subtests for granular reporting
- No external test frameworks (pure stdlib `testing`)

### TypeScript SDK

**No tests exist.** No test files, test configuration, or test scripts were found in `sdk/typescript/`.

## CI Test Infrastructure

### Primary Workflow: `.github/workflows/vorpal.yaml`

The CI pipeline has a multi-stage structure with testing at two levels:

**Stage: `build` (unit tests)**
- Runs `make TARGET=release test` which executes `cargo test --offline --release`
- Covers all Rust workspace crates (cli, config, sdk/rust)
- Runs on 4 matrix runners: `macos-latest`, `macos-latest-large`, `ubuntu-latest`, `ubuntu-latest-arm64`
- Also includes binary verification (dynamic dependency checks via `otool`/`ldd`)

**Stage: `test` (end-to-end build verification)**
- Depends on `build` stage completing
- Downloads built artifacts, sets up a full Vorpal environment via `ALT-F4-LLC/setup-vorpal-action@main`
- Uses S3 registry backend (`altf4llc-vorpal-registry`) with real AWS credentials
- Builds and verifies 6 artifact types across 3 SDKs:
  - `vorpal`, `vorpal-container-image` (Linux only), `vorpal-job`, `vorpal-process`, `vorpal-shell`, `vorpal-user`
  - Each artifact built via Rust SDK first, then Go SDK (`Vorpal.go.toml`), then TypeScript SDK (`Vorpal.ts.toml`)
  - **Cross-SDK determinism check:** Verifies Go and TypeScript SDK produce identical artifact hashes to Rust SDK
- Runs on same 4-runner matrix

**Stage: `vendor` (pre-check)**
- Runs `make TARGET=release check` (`cargo check --offline --release`)
- Validates compilation before code quality and build stages

### Nightly Workflow: `.github/workflows/vorpal-nightly.yaml`

- Scheduled daily at 08:00 UTC via cron
- Creates/recreates a `nightly` tag pointing to current `main` HEAD
- Triggers the primary workflow (which runs the full test suite) via the tag push
- No additional test logic

### Manual Test Scripts

**`script/test/keycloak.sh`** — An interactive OIDC/OAuth2 test script for Keycloak integration:
- Tests device authorization flow, token exchange (worker to artifact, worker to archive), and token introspection
- Requires manual user interaction (browser-based auth) and running Keycloak instance
- Not integrated into CI; used for manual verification of auth flows
- Requires environment variables: `ARCHIVE_CLIENT_SECRET`, `ARTIFACT_CLIENT_SECRET`, `WORKER_CLIENT_SECRET`

## Test Runner and Tooling

| Language | Runner | Framework | Coverage Tool | Mocking |
|----------|--------|-----------|---------------|---------|
| Rust | `cargo test` | Built-in `#[test]` / `#[tokio::test]` | None configured | Hand-rolled mock traits |
| Go | `go test` | Built-in `testing` | None configured | Function variable replacement |
| TypeScript | None | None | None | N/A |

**No coverage tools are configured or run in CI for any language.**

## Test Pyramid Analysis

The current test distribution is inverted relative to a healthy test pyramid:

```
        /  E2E  \        <-- Most investment (CI build verification)
       /----------\
      /   Integration  \  <-- None
     /------------------\
    /     Unit Tests      \  <-- Minimal (~35 tests total)
```

- **Unit tests:** ~7 Rust tests (cache behavior), ~30+ Go tests (config parsing, auth headers)
- **Integration tests:** None. No tests verify component interaction (e.g., gRPC service communication, worker-to-registry flows, SDK-to-CLI integration)
- **End-to-end tests:** CI `test` stage performs full artifact builds across all three SDKs, verifying deterministic output. This is the primary quality gate.

## Gaps and Risks

### Critical Gaps

1. **No Rust SDK tests.** The `vorpal-sdk` crate is published to crates.io and consumed by external users but has zero test coverage. Any regression in the SDK's public API surface (gRPC client wrappers, artifact building, configuration parsing) would only be caught by downstream build failures.

2. **No integration tests.** The system has multiple interacting services (agent, artifact, archive, context, worker — per the protobuf definitions) but no tests verify their interaction. The only integration signal comes from full end-to-end builds in CI.

3. **No coverage measurement.** Without coverage data, there is no way to identify untested code paths or track coverage trends over time.

4. **No TypeScript SDK tests.** The TypeScript SDK is published to npm (`@altf4llc/vorpal-sdk`) with no test coverage.

### Moderate Gaps

5. **No Go SDK tests beyond `config` package.** The Go SDK has ~45 `.go` files across `pkg/artifact/`, `pkg/store/`, `cmd/vorpal/artifact/`, and `pkg/config/`, but tests only exist in `pkg/config/`. The artifact builders, store operations, and command implementations are untested.

6. **Auth flow testing is manual only.** The Keycloak test script (`script/test/keycloak.sh`) requires manual browser interaction and a running Keycloak instance. There are no automated tests for the OIDC device flow, token exchange, or JWT validation logic.

7. **No test isolation for filesystem operations.** The Rust CLI manipulates `/var/lib/vorpal` paths, tar archives, and sandbox environments. These are only exercised in CI end-to-end tests with real system access.

8. **Makefile `test` target is all-or-nothing.** `cargo test` runs all workspace tests together with no granular targeting or parallel test execution configuration.

### Strengths

- **Cross-SDK determinism verification** in CI is a strong end-to-end signal — it catches SDK divergence across Rust, Go, and TypeScript implementations
- **Multi-platform CI matrix** (macOS x86_64, macOS aarch64, Linux x86_64, Linux aarch64) catches platform-specific issues
- **Go tests follow idiomatic patterns** — table-driven tests with subtests provide good coverage of edge cases for the areas they cover
- **Rust cache tests are well-structured** — Given/When/Then pattern, mock traits, and coverage of both positive and negative caching paths
