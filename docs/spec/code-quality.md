---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Documents code quality tooling, conventions, patterns, and gaps across all languages in the vorpal codebase"
owner: "@staff-engineer"
dependencies:
  - architecture.md
---

# Code Quality Specification

## 1. Overview

Vorpal is a polyglot build system with code in three primary languages: Rust (CLI + SDK, ~14,500 LOC), Go (SDK, ~5,200 LOC non-generated), and TypeScript (SDK, ~4,400 LOC non-generated). Additionally, the project defines its API surface via Protocol Buffers (~5 `.proto` files). Code quality tooling varies significantly across these languages — Rust has the strongest enforcement, while Go and TypeScript rely more on convention than automated tooling.

## 2. Language Toolchains

### 2.1 Rust

- **Toolchain**: Pinned via `rust-toolchain.toml` to channel `1.93.1` with `minimal` profile.
- **Required components**: `clippy`, `rust-analyzer`, `rustfmt` (declared in `rust-toolchain.toml`).
- **Edition**: 2021 (declared in all `Cargo.toml` files).
- **Workspace**: Root `Cargo.toml` defines a workspace with members `cli`, `config`, `sdk/rust` using resolver `2`.

### 2.2 Go

- **Version**: Go 1.26.0 (declared in `sdk/go/go.mod`).
- **No linter configuration**: No `.golangci.yml` or equivalent config file exists. No `staticcheck` config.
- **No formatter enforcement**: No `gofmt`/`goimports` check in the makefile targets.

### 2.3 TypeScript

- **Runtime**: Bun (used for tests via `bun test`; `@types/bun` in devDependencies).
- **TypeScript version**: 5.9.3.
- **Compiler strictness**: `tsconfig.json` enables `strict: true`, `forceConsistentCasingInFileNames`, `esModuleInterop`, `skipLibCheck`.
- **Module system**: ES2022 target, Node16 module resolution, ESM (`"type": "module"` in `package.json`).
- **No linter**: No ESLint, Biome, or equivalent configuration exists.
- **No formatter**: No Prettier or equivalent configuration exists.

### 2.4 Protocol Buffers

- **Location**: `sdk/rust/api/` with 5 service definitions (agent, archive, artifact, context, worker).
- **Code generation**: Rust uses `tonic-prost-build` (via `build.rs`), Go uses `protoc` with `protoc-gen-go`/`protoc-gen-go-grpc`, TypeScript uses `ts-proto`.
- **Generation commands**: Defined in the root `makefile` (`generate` target) and in `sdk/typescript/package.json` (`generate:proto` script).
- **No proto linter**: No `buf lint` or equivalent proto linting configuration.

## 3. Linting

### 3.1 Rust — Clippy

Clippy is enforced via the makefile:

```makefile
lint:
    cargo clippy $(CARGO_FLAGS) -- --deny warnings
```

This treats all Clippy warnings as errors. No `clippy.toml` configuration file exists, meaning default Clippy rules are used. A few targeted `#[allow(...)]` annotations exist:

- `#[allow(clippy::too_many_arguments)]` — used in 3 locations for functions with many parameters (context constructors, config resolution).
- `#[allow(dead_code)]` — used in `cli/src/command/start/auth.rs` on 6 struct fields, suggesting partially-implemented or future functionality.

No `rustfmt.toml` exists — default `rustfmt` settings are used.

### 3.2 Go — No Automated Linting

No Go linting targets exist in the makefile. The `vorpal-shell` development environment includes `staticcheck` and `goimports` as available tools (fetched as artifacts), but there is no enforcement mechanism — no CI check, no makefile target.

### 3.3 TypeScript — No Automated Linting

No TypeScript linting configuration or tooling exists. TypeScript's `strict` mode in `tsconfig.json` provides compile-time type checking but no style or pattern enforcement beyond what `tsc` enforces.

## 4. Formatting

### 4.1 Rust — rustfmt

Enforced via the makefile:

```makefile
format:
    cargo fmt --all --check
```

Uses default `rustfmt` configuration (no `rustfmt.toml`). The `--check` flag means this is a verification target, not an auto-fix target.

### 4.2 Go — No Enforcement

No `gofmt` or `goimports` formatting check exists in the makefile or any CI configuration visible in the repository.

### 4.3 TypeScript — No Enforcement

No formatting tool is configured. No `.prettierrc`, `.editorconfig`, or Biome configuration exists.

## 5. Makefile Targets

The root `makefile` provides the primary development workflow. Quality-related targets:

| Target    | Command                                    | Purpose                          |
|-----------|--------------------------------------------|----------------------------------|
| `check`   | `cargo check $(CARGO_FLAGS)`               | Rust type checking (fast)        |
| `format`  | `cargo fmt --all --check`                  | Rust format verification         |
| `lint`    | `cargo clippy $(CARGO_FLAGS) -- --deny warnings` | Rust linting (deny warnings) |
| `test`    | `cargo test $(CARGO_FLAGS)`                | Rust unit tests                  |
| `build`   | `cargo build $(CARGO_FLAGS)`               | Rust compilation                 |

Notably absent:
- No Go-specific quality targets (`go vet`, `staticcheck`, `gofmt`).
- No TypeScript-specific quality targets (`tsc --noEmit`, linting, formatting).
- No proto-specific quality targets (`buf lint`, `buf breaking`).
- No aggregate "ci" or "all-checks" target.

## 6. Error Handling Patterns

### 6.1 Rust

- **Primary pattern**: `anyhow::Result` with `anyhow!()` and `bail!()` for ad-hoc errors throughout the CLI and SDK. The `?` operator is used pervasively for propagation.
- **Typed errors**: `thiserror` is used in two locations for domain-specific errors:
  - `cli/src/command/start/auth.rs` — `AuthError` enum for JWT/OIDC validation errors.
  - `cli/src/command/start/registry.rs` — `BackendError` enum for registry backend errors.
- **No unsafe code**: Zero `unsafe` blocks across the entire Rust codebase.
- **Panics**: Limited use of `.expect()` for truly unrecoverable situations (crypto provider installation, address parsing). Most error paths use `?` propagation.
- **gRPC errors**: `tonic::Status` is used for gRPC-layer errors, with manual conversion from `anyhow` errors in service implementations.

### 6.2 Go

- **Standard Go pattern**: `error` returns with `fmt.Errorf("context: %w", err)` for wrapping. Consistent use of error wrapping with `%w`.
- **Fatal exits**: `log.Fatal` and `log.Fatalf` used in `GetContext()` and similar initialization functions — acceptable for top-level CLI entry points.
- **Validation**: Explicit nil/empty checks on request fields with descriptive `fmt.Errorf` messages (e.g., `"'name' is required"`).

### 6.3 TypeScript

- **Thrown errors**: Standard `throw new Error(...)` pattern.
- **No custom error types**: No structured error hierarchy.
- **Strict null checking**: Enabled via `strict: true` in tsconfig, catching null/undefined issues at compile time.

## 7. Naming Conventions

### 7.1 Rust

- **Modules**: `snake_case` file and module names (e.g., `linux_debian.rs`, `rust_toolchain.rs`, `oci_image.rs`).
- **Types**: `PascalCase` structs and enums (e.g., `ConfigContext`, `ServerBackend`, `ArtifactAlias`).
- **Functions**: `snake_case` (e.g., `get_context`, `build_channel`, `parse_artifact_alias`).
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_NAMESPACE`, `DEFAULT_TAG`).

### 7.2 Go

- **Packages**: Single lowercase words (e.g., `config`, `artifact`, `store`).
- **Exported names**: `PascalCase` (e.g., `ConfigContext`, `BuildClientConn`, `ClientAuthHeader`).
- **Unexported names**: `camelCase` (e.g., `fetchArtifacts`, `parseArtifactAlias`, `isValidComponent`).
- **JSON tags**: `snake_case` (e.g., `json:"access_token"`, `json:"client_id"`).

### 7.3 TypeScript

- **Files**: `camelCase` (e.g., `artifact.ts`, `context.ts`, `vorpal.ts`).
- **Classes**: `PascalCase` (e.g., `ConfigContext`, `Rust`, `OciImage`).
- **Functions**: `camelCase` (e.g., `buildVorpal`, `getGoarch`, `parseArtifactAlias`).
- **Constants**: `SCREAMING_SNAKE_CASE` for arrays (e.g., `SYSTEMS`).

### 7.4 Protobuf

- **Packages**: Dot-separated (e.g., `vorpal.artifact`, `vorpal.agent`).
- **Services**: `PascalCase` with `Service` suffix (e.g., `ArtifactService`, `ArchiveService`).
- **Messages**: `PascalCase` (e.g., `ArtifactStep`, `PrepareArtifactRequest`).
- **Enums**: `SCREAMING_SNAKE_CASE` values (e.g., `UNKNOWN_SYSTEM`, `AARCH64_LINUX`).
- **Fields**: `snake_case` (e.g., `artifact_digest`, `artifact_namespace`).

## 8. Design Patterns

### 8.1 Builder Pattern

Used extensively across all three SDK languages for constructing artifacts:

- **Rust**: Method chaining not present; construction uses struct literals directly.
- **Go**: Not present; direct struct initialization with validation in `AddArtifact`.
- **TypeScript**: Fluent builder pattern with `withArtifacts()`, `withEnvironments()`, `withAliases()`, `withBins()`, etc. — the most ergonomic SDK.

### 8.2 Configuration Pattern

Multi-source configuration resolution in the CLI:

- User config (`~/.vorpal/settings.json`) + project config (`Vorpal.toml`) + CLI flags, with explicit precedence: CLI flags > project config > user config > built-in defaults.
- Configuration language is TOML-based (`Vorpal.toml`, `Vorpal.go.toml`, `Vorpal.ts.toml`).

### 8.3 gRPC Service Pattern

Consistent across Rust and Go:

- Proto definitions are the source of truth in `sdk/rust/api/`.
- Code is generated into each language's SDK.
- Client creation follows the same pattern: parse URI scheme, configure TLS/insecure transport, build client connection.
- Server implementations use `tonic` (Rust) and `google.golang.org/grpc` (Go).

### 8.4 Cross-Language Parity

The Rust and Go SDKs maintain functional parity on core operations (artifact management, auth, context serving). Comments in the Go code explicitly reference the Rust implementation (e.g., `"This matches the Rust SDK's client_auth_header function"`). TypeScript SDK focuses on artifact definition and building rather than server-side operations.

## 9. Testing

### 9.1 Current State

- **Rust**: One `#[cfg(test)]` module found (`cli/src/command/start/registry.rs`). `tempfile` is listed as a dev-dependency in `cli/Cargo.toml`. Test coverage is minimal.
- **Go**: Two test files exist in `sdk/go/pkg/config/`:
  - `context_test.go` — comprehensive table-driven tests for `parseArtifactAlias` (~60 test cases).
  - `context_auth_test.go` — auth-related tests (exists but not examined in detail).
- **TypeScript**: No test files found (`__tests__/` is excluded in `tsconfig.json` but the directory doesn't exist). `bun test` is configured as a script.
- **Makefile**: `make test` runs `cargo test` only — no Go or TypeScript test execution.

### 9.2 Gaps

- No integration test infrastructure.
- No end-to-end test framework.
- Rust test coverage is extremely sparse relative to codebase size (~14,500 LOC with ~1 test module).
- TypeScript has zero tests despite having a test runner configured.
- No test coverage reporting.

## 10. CI/CD

No `.github/workflows/` directory exists in the repository. No CI configuration files (e.g., `.gitlab-ci.yml`, `Jenkinsfile`, `.circleci/config.yml`) were found. This means:

- Linting, formatting, and test checks are enforced only via manual `make` commands.
- No automated quality gates exist for pull requests.
- No build verification on merge.

**Note**: CI may exist outside this repository (e.g., in a separate infrastructure repo or in Vorpal's own build system), but it is not visible within this codebase.

## 11. Editor Configuration

- **No `.editorconfig`**: No cross-editor settings for indentation, line endings, etc.
- **Rust analyzer**: Included as a toolchain component via `rust-toolchain.toml` — IDE support is available for Rust developers.
- **TypeScript**: `tsconfig.json` provides IDE type-checking support.
- **Go**: `gopls` is available as a development environment artifact but no configuration exists in the repo.

## 12. Dependency Management

### 12.1 Rust

- **Cargo**: Standard `Cargo.toml` / `Cargo.lock` workflow.
- **Vendoring**: `make vendor` target runs `cargo vendor --versioned-dirs` for offline/reproducible builds.
- **cargo-machete**: `[package.metadata.cargo-machete]` in `sdk/rust/Cargo.toml` with `ignored` list — suggests the tool is used for detecting unused dependencies.
- **Version pinning**: Dependencies use exact minor versions (e.g., `"1.0.100"` not `"1.0"`), which is good for reproducibility.

### 12.2 Go

- **Go modules**: `go.mod` / `go.sum` with minimal dependencies (4 direct, 4 indirect).
- **No dependency auditing tool configured**.

### 12.3 TypeScript

- **Bun**: `bun.lock` lockfile.
- **Minimal dependencies**: 3 runtime (`@bufbuild/protobuf`, `@grpc/grpc-js`, `smol-toml`), 3 dev (`@types/bun`, `ts-proto`, `tsx`, `typescript`).
- **No dependency auditing tool configured**.

## 13. Identified Gaps

| Area | Gap | Impact |
|------|-----|--------|
| Go linting | No `golangci-lint` or `staticcheck` enforcement | Style inconsistencies, missed bugs |
| Go formatting | No `gofmt` check in makefile | Potential formatting drift |
| TypeScript linting | No ESLint or Biome | No pattern enforcement beyond type checking |
| TypeScript formatting | No Prettier or equivalent | No formatting consistency guarantee |
| Proto linting | No `buf lint` | No schema style enforcement |
| CI/CD | No visible CI pipeline | No automated quality gates |
| Editor config | No `.editorconfig` | Cross-editor inconsistency risk |
| Test coverage | Minimal Rust tests, zero TypeScript tests | Low confidence in correctness |
| Aggregate quality target | No unified `make ci` or `make check-all` | Easy to skip non-Rust checks |
| Clippy config | No `clippy.toml` | Using only default rules; no project-specific tuning |
| rustfmt config | No `rustfmt.toml` | Using defaults; no project-specific style preferences documented |
