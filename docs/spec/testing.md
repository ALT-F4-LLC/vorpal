# Testing Specification

This document describes the testing strategy, infrastructure, and coverage that **currently exists**
in the Vorpal codebase. It is derived from direct inspection of the repository and reflects the
state of the `feature/typescript-support` branch.

---

## 1. Test Landscape Overview

Vorpal is a multi-language project with three SDK implementations (Rust, Go, TypeScript) and a
Rust CLI/server. Testing is spread across four distinct ecosystems:

| Component | Language | Test Runner | Test Location |
|---|---|---|---|
| CLI (`cli/`) | Rust | `cargo test` | Inline `#[cfg(test)]` modules |
| Rust SDK (`sdk/rust/`) | Rust | `cargo test` | Inline `#[cfg(test)]` modules |
| Go SDK (`sdk/go/`) | Go | `go test` | `*_test.go` files alongside source |
| TypeScript SDK (`sdk/typescript/`) | TypeScript | `bun test` | `src/__tests__/*.test.ts` |
| Integration tests (`tests/`) | Bash | Shell script | `tests/typescript-integration.sh` |
| Auth test script (`script/test/`) | Bash | Shell script | `script/test/keycloak.sh` |

The top-level `make test` target runs `cargo test` across the Rust workspace (CLI, config, Rust
SDK). Go and TypeScript tests are run separately.

---

## 2. Test Pyramid Breakdown

### 2.1 Unit Tests

Unit tests form the majority of the test suite. All three SDKs have unit tests covering core
domain logic.

#### Rust SDK (`sdk/rust/src/context.rs`)

- **38 tests** in a single `#[cfg(test)]` module
- Focus: Artifact alias parsing (`parse_artifact_alias`)
- Pattern: Table-driven tests using helper functions `assert_alias()` and `assert_alias_error()`
- Covers: basic formats, real-world aliases, edge cases (multiple colons, special characters,
  numeric components), error conditions (empty input, too long, invalid characters), default
  value application, character validation, and security-relevant cases (path traversal, shell
  metacharacters, control characters)
- Cross-SDK parity: These tests serve as the **reference implementation** that Go and TypeScript
  tests are ported from

#### Rust CLI (`cli/src/command/start/registry.rs`)

- **7 async tests** (`#[tokio::test]`) in a `#[cfg(test)]` module
- Focus: Archive server caching behavior
- Pattern: Mock-based testing with `MockBackend` implementing `ArchiveBackend` trait
- Covers: cache hits, cache misses (different keys, different namespaces), negative caching,
  TTL=0 disabling cache, TTL expiration, empty digest validation
- Uses: `std::sync::atomic::AtomicUsize` for call counting, `Arc` for shared mock state

#### Go SDK (`sdk/go/pkg/config/`)

- **2 test files** with standard Go `testing` package:
  - `context_test.go`: Artifact alias parsing (table-driven, mirrors Rust tests)
  - `context_auth_test.go`: Client authentication header construction
- `context_test.go` covers: basic formats, real-world aliases, edge cases, error conditions,
  default value application (mirrors Rust test suite)
- `context_auth_test.go` covers: no credentials file, valid credentials, registry not found,
  issuer not found, invalid JSON, multiple registries, path helper functions
- Pattern: Table-driven tests (`[]struct` + `t.Run`), mock via function variable replacement
  (`getKeyCredentialsPathFunc`), temp directories via `t.TempDir()`

#### TypeScript SDK (`sdk/typescript/src/__tests__/`)

- **9 test files** with `bun:test` runner:

  | File | Tests | Focus |
  |---|---|---|
  | `alias.test.ts` | ~50 | Alias parsing/formatting, round-trip validation |
  | `artifact.test.ts` | ~15 | `ArtifactBuilder`, `ArtifactSourceBuilder`, `ArtifactStepBuilder` |
  | `cli.test.ts` | ~15 | CLI argument parsing (happy path + error cases) |
  | `context.test.ts` | ~8 | `TestStore` simulating `ConfigContext` behavior |
  | `digest-parity.test.ts` | ~15 | Golden test vectors for cross-SDK digest compatibility |
  | `sdk-exports.test.ts` | ~13 | Validates all public API exports from `@vorpal/sdk` |
  | `sdk-parity.test.ts` | ~10 | Cross-SDK parity framework (determinism, field sensitivity) |
  | `step-parity.test.ts` | ~20 | `bash()`, `bwrap()`, `docker()` step construction parity |
  | `template.test.ts` | ~20 | Template file existence, syntax, structure, substitution |

- Pattern: `describe`/`test` blocks, `expect` assertions, Bun transpiler for syntax validation
- Notable: `digest-parity.test.ts` uses **golden test vectors** -- hardcoded JSON strings and
  SHA-256 digests verified against Rust `serde_json::to_vec` + `sha256::digest` output

### 2.2 Integration Tests

#### Shell-based integration (`tests/typescript-integration.sh`)

- **2 test sections**:
  1. Delegates to `bun test` for TypeScript SDK unit tests
  2. Cross-SDK parity: builds the same artifact with Rust, Go, and TypeScript configs and
     compares SHA-256 digests
- Supports `--quick` mode (skips tests requiring Vorpal services)
- Prerequisites: Vorpal services running, cargo built, config files present
- The cross-SDK parity section is partially implemented (TODO: `Vorpal.ts.toml` needs identical
  artifact definitions to Rust/Go configs)

#### CI pipeline integration (`vorpal.yaml`)

- The `test` job in CI performs **end-to-end cross-SDK parity validation**:
  - Builds artifacts with Rust config (`Vorpal.toml`) and Go config (`Vorpal.go.toml`)
  - Compares resulting digests to verify both SDKs produce identical outputs
  - Runs on 4 platforms: `macos-latest`, `macos-latest-large`, `ubuntu-latest`, `ubuntu-latest-arm64`
  - Validates 6 artifact types: `vorpal`, `vorpal-container-image` (Linux only), `vorpal-job`,
    `vorpal-process`, `vorpal-shell`, `vorpal-user`
- TypeScript is **not yet integrated** into the CI parity test matrix

#### Keycloak test script (`script/test/keycloak.sh`)

- Shell script for testing OAuth2/OIDC authentication flow with Keycloak
- Not automated in CI; used for manual testing of auth features
- Requires environment variables: `ARCHIVE_CLIENT_SECRET`, `ARTIFACT_CLIENT_SECRET`,
  `WORKER_CLIENT_SECRET`

### 2.3 End-to-End Tests

- The CI `test` job is the closest thing to e2e testing: it builds the CLI binary, starts Vorpal
  services, builds real artifacts, and compares cross-SDK outputs
- No dedicated e2e test framework or test harness exists
- The `tests/typescript-integration.sh` script provides a local equivalent but requires manual
  setup

---

## 3. Test Runners and Configuration

### 3.1 Rust (`cargo test`)

- **Runner**: Built-in `cargo test` (no external test framework)
- **Configuration**: None beyond standard Cargo workspace settings
- **Workspace**: Tests run across all workspace members (`cli`, `config`, `sdk/rust`)
- **Invocation**: `make test` or `cargo test` at the project root
- **CI step**: `./script/dev.sh make TARGET=release test` (runs in the build job)
- **Dev dependency**: `tempfile = "3.24.0"` in `cli/Cargo.toml` (for temporary file/directory
  management in tests)
- **Async support**: `#[tokio::test]` for async test functions

### 3.2 Go (`go test`)

- **Runner**: Standard `go test`
- **Configuration**: None; standard Go conventions
- **Invocation**: `cd sdk/go && go test ./...`
- **CI integration**: Not present in CI workflows (gap)
- **Mocking pattern**: Function variable replacement (e.g., `getKeyCredentialsPathFunc`)

### 3.3 TypeScript (`bun test`)

- **Runner**: Bun's built-in test runner (`bun test`)
- **Configuration**: `tsconfig.json` excludes `src/__tests__` from compilation output
- **Test directory**: `sdk/typescript/src/__tests__/`
- **Package script**: `"test": "bun test"` in `package.json`
- **Invocation**: `cd sdk/typescript && bun test`
- **CI integration**: Indirectly via `tests/typescript-integration.sh` (not directly in CI
  workflow)
- **Imports**: Tests use `bun:test` for `describe`, `expect`, `test`, `beforeEach`, `afterEach`

### 3.4 Shell scripts

- **Runner**: Bash with `set -euo pipefail`
- **Pattern**: Custom pass/fail/skip counters, `section()` helper for output grouping
- **Exit code**: 0 on all pass, 1 on any failure

---

## 4. Coverage Tools

**There are no code coverage tools configured in the project.**

- No `tarpaulin`, `llvm-cov`, `grcov`, or `cargo-llvm-cov` for Rust
- No `go test -coverprofile` configuration or coverage reporting for Go
- No `bun test --coverage` or equivalent for TypeScript
- No coverage thresholds, coverage badges, or coverage reporting in CI
- No Codecov, Coveralls, or similar coverage service integration

This is a significant gap. There is no quantitative measure of how much code is exercised by tests.

---

## 5. Test Patterns and Conventions

### 5.1 Cross-SDK Parity Testing

The most distinctive testing pattern in this project is **cross-SDK digest parity**. The core
invariant is: identical artifact definitions across Rust, Go, and TypeScript SDKs must produce
byte-for-byte identical JSON serialization and therefore identical SHA-256 digests.

This is enforced at multiple levels:

1. **Golden test vectors** (`digest-parity.test.ts`): Hardcoded expected JSON strings and digests
   verified against Rust reference output
2. **Field order tests**: Verify JSON keys appear in protobuf field number order
3. **Serialization edge cases**: Enums as integers (not strings), `undefined`/`None` as `null`,
   empty arrays serialized (not omitted)
4. **CI parity builds**: Same artifact built with Rust and Go configs, digests compared
5. **Step parity tests** (`step-parity.test.ts`): Verify `bash()`, `bwrap()`, `docker()` produce
   identical step structures across SDKs

### 5.2 Table-Driven Tests

All three SDKs use table-driven test patterns for alias parsing:
- Rust: `assert_alias()` / `assert_alias_error()` helpers with individual `#[test]` functions
- Go: `[]struct` + `t.Run()` subtests (idiomatic Go table-driven testing)
- TypeScript: `assertAlias()` / `assertAliasError()` helpers with Bun `test()` blocks

The test cases are **intentionally identical across all three SDKs**, with comments referencing
the source tests they were ported from (e.g., "ported from Rust sdk/rust/src/context.rs lines
712-1017").

### 5.3 Mock Patterns

- **Rust**: Trait-based mocking (`MockBackend` implements `ArchiveBackend`), with `AtomicUsize`
  for call counting and `Arc` for shared state
- **Go**: Function variable replacement (`getKeyCredentialsPathFunc`), `t.TempDir()` for temp
  files, `json.Marshal` for creating test fixtures
- **TypeScript**: `TestStore` class simulating `ConfigContext` behavior (in-memory artifact
  store), `beforeEach`/`afterEach` for environment variable isolation in CLI tests

### 5.4 Test Naming Conventions

- **Rust**: `test_` prefix with snake_case descriptive names (e.g., `test_cache_hit_skips_backend`)
- **Go**: `Test` prefix with PascalCase (e.g., `TestClientAuthHeaderValid`)
- **TypeScript**: String descriptions in `test()` blocks (e.g., `"bash step PATH construction
  with artifacts"`)

---

## 6. CI Test Pipeline

The CI pipeline (`vorpal.yaml`) has the following test-related stages:

```
vendor -> code-quality -> build (includes cargo test) -> test (cross-SDK parity)
```

### 6.1 Stage Details

| Stage | What it tests | Platforms |
|---|---|---|
| `vendor` | `cargo check` (compilation) | 4 platforms |
| `code-quality` | `cargo fmt --check`, `cargo clippy --deny warnings` | macOS only |
| `build` | `cargo test` (all Rust unit tests) | 4 platforms |
| `test` | Cross-SDK parity (Rust vs Go digest comparison) | 4 platforms |

### 6.2 What is NOT in CI

- Go SDK tests (`go test ./...`)
- TypeScript SDK tests (`bun test`)
- TypeScript integration tests (`tests/typescript-integration.sh`)
- Code coverage collection or reporting
- TypeScript cross-SDK parity validation (only Rust vs Go is tested)

---

## 7. How to Run Tests

### All Rust tests (workspace-wide)

```bash
make test
# or
cargo test
```

### Go SDK tests

```bash
cd sdk/go && go test ./...
```

### TypeScript SDK tests

```bash
cd sdk/typescript && bun test
```

### TypeScript integration tests

```bash
# Quick mode (no Vorpal services required)
./tests/typescript-integration.sh --quick

# Full mode (requires Vorpal services running)
make vorpal-start  # in another terminal
./tests/typescript-integration.sh
```

### Rust linting and formatting (code quality checks)

```bash
make format  # cargo fmt --all --check
make lint    # cargo clippy -- --deny warnings
```

---

## 8. Gaps and Recommendations

### 8.1 Coverage Gaps

| Gap | Severity | Description |
|---|---|---|
| No coverage tooling | High | No way to measure which code paths are exercised by tests |
| Go SDK not in CI | Medium | Go tests exist but are not run in the CI pipeline |
| TypeScript SDK not in CI | Medium | TypeScript tests exist but are not run in the CI pipeline |
| TypeScript not in parity matrix | Medium | CI validates Rust vs Go parity but not TypeScript |
| Config crate has no tests | Medium | `config/` has zero test functions despite containing artifact build logic |
| CLI has limited test coverage | Medium | Only `registry.rs` has tests; other CLI commands untested |
| No e2e test framework | Low | E2e testing is ad-hoc via CI steps and shell scripts |
| Keycloak auth tests manual only | Low | `script/test/keycloak.sh` is not automated |

### 8.2 What is Well-Tested

| Area | Assessment |
|---|---|
| Artifact alias parsing | Excellent -- identical tests across all three SDKs |
| Digest parity / serialization | Excellent -- golden vectors, field ordering, edge cases |
| Step construction (TypeScript) | Good -- covers `bash()`, `bwrap()`, `docker()` thoroughly |
| CLI argument parsing (TypeScript) | Good -- happy path and error cases |
| Archive caching (Rust) | Good -- mock-based, covers TTL, expiration, edge cases |
| Go auth credentials | Good -- covers multiple scenarios with filesystem mocking |
| Template validation (TypeScript) | Good -- file existence, syntax, structure, substitution |
| SDK public API surface | Good -- validates all expected exports |

### 8.3 Testing Debt

1. The `config/` crate (Rust) defines all artifact build logic (vorpal, vorpal-job,
   vorpal-process, vorpal-shell, vorpal-user, vorpal-container-image, vorpal-release) but has
   **zero unit tests**. These artifacts are only tested indirectly via the CI e2e parity builds.

2. The CLI crate has extensive source code across multiple command modules but only the
   `registry.rs` module has tests. Commands like `build`, `init`, `inspect`, `login`, and
   `system` are untested at the unit level.

3. TypeScript cross-SDK parity is partially implemented. The `Vorpal.ts.toml` config (referenced
   in `tests/typescript-integration.sh`) exists in the project root but is not yet tested in CI.

4. The Go and TypeScript test suites run successfully locally but are not gated in CI, meaning
   regressions in these SDKs could ship undetected.
