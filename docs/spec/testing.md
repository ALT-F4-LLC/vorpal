# Testing Specification

## Overview

Vorpal's testing strategy is minimal and focused on specific high-value areas. The project relies
heavily on CI-level integration and end-to-end testing (SDK parity checks, artifact build
validation) rather than extensive unit test suites. Test coverage is sparse across most of the
codebase, with tests concentrated in the Go SDK config package and the Rust CLI registry server.

## Test Pyramid

### Unit Tests

**Rust (CLI crate)**

- **Location**: `cli/src/command/start/registry.rs` (inline `#[cfg(test)]` module)
- **Runner**: `cargo test` (standard Rust test harness)
- **Test count**: 7 async tests using `#[tokio::test]`
- **Focus**: Archive server caching behavior — cache hits, misses, TTL expiration, negative
  caching, TTL=0 bypass, and input validation (empty digest)
- **Mocking pattern**: Hand-rolled `MockBackend` struct implementing `ArchiveBackend` trait, using
  `Arc<AtomicUsize>` for call-count tracking. No external mocking framework.
- **Dev dependencies**: `tempfile = 3.24.0` (declared in `cli/Cargo.toml`)

**Rust (SDK and Config crates)**

- No unit tests exist in `sdk/rust/` or `config/`.

**Go SDK**

- **Location**: `sdk/go/pkg/config/context_test.go`, `sdk/go/pkg/config/context_auth_test.go`
- **Runner**: `go test` (standard Go testing package)
- **Test count**: 9 test functions across 2 files
- **Focus**:
  - `context_test.go`: `parseArtifactAlias` — table-driven tests covering basic formats,
    real-world examples, edge cases (multiple colons, special chars, numeric components), and
    error cases (empty string, empty tag, too many slashes, too-long alias). Also tests default
    value application.
  - `context_auth_test.go`: `ClientAuthHeader` — tests credential file missing, valid
    credentials, registry not found, issuer not found, invalid JSON, multiple registries, and
    path helper functions.
- **Mocking pattern**: Function variable replacement (`getKeyCredentialsPathFunc`) with deferred
  restore. Uses `t.TempDir()` for filesystem isolation. No external mocking framework.
- **Test data**: Inline structs marshaled to JSON files in temp directories. No shared fixtures
  or testdata directories.

**TypeScript SDK**

- No tests exist in `sdk/typescript/`.

### Integration Tests

There are no dedicated integration test suites (no `tests/` directories, no integration test
files). The Rust workspace has no `[[test]]` entries in any `Cargo.toml`.

### End-to-End Tests

E2E testing is performed at two levels:

**1. CI Pipeline (`.github/workflows/vorpal.yaml`)**

The `test` job runs after `build` and exercises the full system:

- Sets up Vorpal services with S3 registry backend (`ALT-F4-LLC/setup-vorpal-action@main`)
- Builds artifacts using the Rust SDK (`vorpal build "vorpal"`, etc.)
- Builds the same artifacts using the Go SDK (`--config "Vorpal.go.toml"`) and compares digests
- Builds the same artifacts using the TypeScript SDK (`--config "Vorpal.ts.toml"`) and compares
  digests
- Validates cross-SDK parity: the same artifact name must produce identical digests across all
  three SDKs
- Tests run across 4 matrix runners: `macos-latest`, `macos-latest-large`, `ubuntu-latest`,
  `ubuntu-latest-arm64`
- Container image builds are tested only on Ubuntu runners

Artifacts tested:
- `vorpal`, `vorpal-container-image` (Linux only), `vorpal-job`, `vorpal-process`, `vorpal-shell`,
  `vorpal-user`

**2. Claude Code Skills**

- `/e2e-test`: Manual skill that starts Vorpal services, builds an artifact, and reports
  pass/fail. Uses `make vorpal-start` and `make vorpal`.
- `/sdk-parity`: Manual skill that runs Rust SDK build then Go SDK build for the same artifact
  and compares digests.

## Test Runners and Tools

| Tool | Purpose | Configuration |
|------|---------|---------------|
| `cargo test` | Rust unit tests | `makefile` target `test` — runs `cargo test $(CARGO_FLAGS)` |
| `go test` | Go unit tests | Standard `go test ./...` (no custom config) |
| `cargo fmt --all --check` | Rust formatting check | `makefile` target `format` |
| `cargo clippy -- --deny warnings` | Rust linting | `makefile` target `lint` |
| `cargo check` | Rust type checking | `makefile` target `check` |

## CI Pipeline Test Stages

The `.github/workflows/vorpal.yaml` pipeline runs on every PR and push to `main`:

```
vendor → code-quality → build → test → [container-image → container-image-manifest]
                                     → [release]
```

1. **vendor**: `cargo check` (release mode) across 4 platform matrix
2. **code-quality**: `cargo fmt --all --check` + `cargo clippy --deny warnings`
3. **build**: `cargo build` + `cargo test` + `cargo dist` (release mode, 4 platforms)
4. **test**: Full E2E — Vorpal services + artifact builds + cross-SDK parity checks (4 platforms)
5. **container-image** / **release**: Tag-triggered only (not part of regular test flow)

The nightly workflow (`.github/workflows/vorpal-nightly.yaml`) creates a nightly tag from `main`
— it does not run additional tests.

## Coverage

- **No coverage tooling is configured.** There is no `cargo-tarpaulin`, `cargo-llvm-cov`,
  `go test -cover`, or any coverage reporting in CI.
- **No coverage thresholds or gates.**

## Test Infrastructure

### Build System

- `makefile` provides all test-related targets: `check`, `format`, `lint`, `build`, `test`, `dist`
- `script/dev.sh` bootstraps the development environment (xz, amber, rustup, protoc, terraform)
- `script/dev/debian.sh` installs system dependencies on Debian/Ubuntu CI runners

### External Services for Testing

- **Keycloak** (`docker-compose.yaml`): Local Keycloak instance for auth testing
  (`quay.io/keycloak/keycloak:26.5.4`)
- `script/test/keycloak.sh`: Interactive script for testing OAuth2 device authorization flow,
  token exchange, and token introspection against local Keycloak. Not automated — requires manual
  browser interaction.
- **AWS S3**: CI uses real S3 bucket (`altf4llc-vorpal-registry`) for registry backend in E2E
  tests.

### Rust Toolchain

- Pinned to Rust `1.93.1` via `rust-toolchain.toml`
- Components: `clippy`, `rust-analyzer`, `rustfmt`

## Mocking Patterns

- **Rust**: Hand-rolled mock structs implementing traits. Uses `Arc<AtomicUsize>` for call
  tracking. Trait-based design (`ArchiveBackend`) enables mock injection.
- **Go**: Function variable swapping (`getKeyCredentialsPathFunc`) with `defer` restore. Uses
  `t.TempDir()` and `os.WriteFile` for filesystem mocking.
- **No external mocking frameworks** in either language.

## Gaps and Observations

1. **Low unit test coverage**: Only 2 Rust source files and 2 Go source files have tests. The
   vast majority of application logic (CLI commands, SDK artifact builders, gRPC services,
   config parsing, store operations) is untested at the unit level.
2. **No integration tests**: No dedicated integration test suites exist. The gap between unit
   tests and full E2E (which requires running services + S3) is large.
3. **No TypeScript SDK tests**: The TypeScript SDK has zero test coverage.
4. **No Rust SDK tests**: The `vorpal-sdk` Rust crate has no tests despite containing core
   business logic (artifact definitions, API types, build orchestration).
5. **No `vorpal-config` tests**: The config binary crate has no tests.
6. **No coverage tooling**: No way to measure or enforce test coverage.
7. **No test data directories**: No shared fixtures, testdata, or golden files.
8. **No property-based testing**: No use of proptest, quickcheck, or similar.
9. **No benchmark tests**: No `#[bench]` or criterion benchmarks.
10. **Auth flow testing is manual**: The Keycloak test script (`script/test/keycloak.sh`) requires
    interactive browser input and cannot run in CI.
11. **E2E tests depend on external infrastructure**: CI E2E requires AWS S3 credentials and the
    `setup-vorpal-action` GitHub Action. No local-only E2E path exists for contributors without
    AWS access.
12. **SDK parity is the primary quality gate**: Cross-SDK digest comparison is the most rigorous
    automated test — it validates that all three SDK implementations produce identical build
    artifacts. This is effectively the project's integration test.

## How to Run Tests

### Local Development

```bash
# Run Rust unit tests
make test

# Run with release flags
make TARGET=release test

# Run Go SDK tests
cd sdk/go && go test ./...

# Run formatting check
make format

# Run linter
make lint

# Run type check
make check
```

### E2E Testing (requires running services)

```bash
# Start Vorpal services
make vorpal-start

# Build an artifact (in another terminal)
make vorpal

# Or with a specific artifact
make VORPAL_ARTIFACT="vorpal-shell" vorpal
```

### SDK Parity Testing (requires running services)

```bash
# Rust SDK build
make VORPAL_ARTIFACT="vorpal" vorpal

# Go SDK build (compare digest with Rust)
make VORPAL_ARTIFACT="vorpal" VORPAL_FLAGS="--config 'Vorpal.go.toml'" vorpal

# TypeScript SDK build (compare digest with Rust)
make VORPAL_ARTIFACT="vorpal" VORPAL_FLAGS="--config 'Vorpal.ts.toml'" vorpal
```
