---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Code quality tooling, conventions, and enforcement across the Vorpal polyglot codebase"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - testing.md
---

# Code Quality

This document describes the code quality tooling, naming conventions, error handling patterns, and enforcement mechanisms that exist in the Vorpal project today.

## 1. Language Ecosystem Overview

Vorpal is a polyglot project with three primary implementation languages:

| Language   | Role                          | Toolchain Version | Package Manager |
|------------|-------------------------------|-------------------|-----------------|
| Rust       | CLI, SDK, core runtime        | 1.93.1 (pinned)   | Cargo           |
| Go         | SDK, config runner            | 1.26.0            | Go modules      |
| TypeScript | SDK, config runner            | 5.9.3             | Bun / npm       |
| Protobuf   | API contract definitions      | N/A (external)    | protoc          |

## 2. Linting and Formatting

### 2.1 Rust

- **Formatter**: `rustfmt` -- enforced via `cargo fmt --all --check` in both the `makefile` (`make format`) and the CI `code-quality` job. No custom `.rustfmt.toml` exists; the project uses default `rustfmt` settings.
- **Linter**: `clippy` -- invoked with `cargo clippy -- --deny warnings` (via `make lint`). All clippy warnings are treated as errors. No custom `clippy.toml` configuration exists; default clippy rules apply.
- **Toolchain components**: Both `clippy`, `rust-analyzer`, and `rustfmt` are declared in `rust-toolchain.toml` and automatically installed with the pinned Rust version.
- **Selective suppressions**: A small number of `#[allow(...)]` annotations exist in the codebase:
  - `#[allow(clippy::too_many_arguments)]` -- used in `sdk/rust/src/context.rs`, `config/src/artifact.rs`, and build script generators where function signatures are inherently complex.
  - `#[allow(dead_code)]` -- used in `cli/src/command/start/auth.rs` for struct fields that are parsed but not yet fully consumed.

### 2.2 Go

- **No explicit linter configuration** (no `.golangci.yml`, no `staticcheck.toml`). The Go SDK does not have `make` targets for linting or formatting in the top-level `makefile`.
- **Gap**: Go code quality is not enforced in CI. There are no `go vet`, `golangci-lint`, or `gofmt` checks in the GitHub Actions workflows.
- The Go SDK does include `staticcheck` as an artifact dependency (for Vorpal's own build system), but it is not applied to the Go SDK source code itself in CI.

### 2.3 TypeScript

- **Compiler strictness**: `tsconfig.json` enables `"strict": true`, `"forceConsistentCasingInFileNames": true`, and targets ES2022 with Node16 module resolution.
- **No explicit linter**: No ESLint, Biome, or Prettier configuration exists. Code quality relies on TypeScript's strict compiler checks only.
- **Gap**: TypeScript code quality (beyond type checking) is not enforced in CI. The CI workflow runs `bun run build` (which invokes `tsc`) but there is no dedicated lint step.

### 2.4 Protobuf

- Proto files use `proto3` syntax with package namespacing (`vorpal.agent`, `vorpal.artifact`, etc.).
- `go_package` option is set on all proto files for Go code generation.
- No proto linter (e.g., `buf lint`) is configured.

## 3. CI Enforcement

The GitHub Actions workflow (`.github/workflows/vorpal.yaml`) enforces code quality through a dedicated `code-quality` job that gates the `build` job:

```
vendor -> code-quality -> build -> test -> release
```

The `code-quality` job runs on `macos-latest` only and executes:

1. `make format` -- `cargo fmt --all --check` (fail on unformatted Rust code)
2. `make TARGET=release lint` -- `cargo clippy --offline --release -- --deny warnings`

**Scope**: Only Rust code is covered by CI quality gates. Go and TypeScript code quality is not enforced in the pipeline.

## 4. Naming Conventions

### 4.1 Rust

- **Crate names**: Hyphenated lowercase (`vorpal-cli`, `vorpal-sdk`, `vorpal-config`).
- **Module organization**: Nested `mod` hierarchy mirroring directory structure. Subcommands organized under `command/` with one file per subcommand.
- **Functions/methods**: `snake_case` throughout, following standard Rust conventions.
- **Types**: `PascalCase` for structs and enums. Enum variants are `PascalCase` (e.g., `CommandSystem::Keys`, `ArtifactSystem::Aarch64Darwin`).
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_NAMESPACE`, `DEFAULT_TAG`).
- **Prefix patterns**: Getter methods use `get_` prefix consistently (e.g., `get_artifact_store()`, `get_system()`).

### 4.2 Go

- **Package names**: Short, lowercase, single-word (e.g., `config`, `artifact`, `store`).
- **Struct fields**: `PascalCase` for exported, `camelCase` for unexported. JSON tags use `snake_case` (e.g., `json:"access_token"`).
- **Functions**: `PascalCase` for exported, `camelCase` for unexported. Follows standard Go conventions.

### 4.3 TypeScript

- **Module system**: ESM (`"type": "module"` in `package.json`).
- **File naming**: `snake_case` for generated proto files, `camelCase` for authored files (e.g., `artifact.ts`, `context.ts`).

### 4.4 Protobuf

- **Package naming**: Dot-separated lowercase (`vorpal.artifact`, `vorpal.agent`).
- **Message fields**: `snake_case` field names (e.g., `artifact_namespace`, `artifact_context`).
- **Enum values**: `SCREAMING_SNAKE_CASE` with type prefix (e.g., `UNKNOWN_SYSTEM`, `AARCH64_LINUX`).
- **Service methods**: `PascalCase` (e.g., `GetArtifact`, `StoreArtifact`).

## 5. Error Handling Patterns

### 5.1 Rust

- **Primary error type**: `anyhow::Result` used pervasively across all crates. Functions return `Result<T>` (aliased from `anyhow`).
- **Error creation**: `anyhow!()` macro for ad-hoc errors, `bail!()` for early returns with error messages.
- **Context chaining**: `.with_context(|| ...)` used for adding context to I/O and network operations (e.g., file reads, gRPC connections).
- **Domain-specific errors**: `thiserror` is used selectively for typed error enums in specific domains:
  - `cli/src/command/start/auth.rs` -- `AuthError` enum for authentication failures (invalid scheme, missing kid, validation failure).
  - `cli/src/command/start/registry.rs` -- `RegistryError` enum for registry configuration errors.
- **Panic usage**: `.expect()` is used in a small number of initialization paths (crypto provider install, address parsing) where failure is unrecoverable. Production code paths use `?` propagation.
- **gRPC error mapping**: `tonic::Status` used for gRPC error responses with appropriate codes (`InvalidArgument`, `NotFound`).

### 5.2 Go

- **Standard Go error handling**: Functions return `(T, error)` tuples. Errors are checked with `if err != nil` patterns.
- **Error creation**: `fmt.Errorf()` for formatted error messages. No use of error wrapping with `%w` observed in the sampled code.
- **Fatal exits**: `log.Fatalf()` used in some paths for unrecoverable errors.

### 5.3 TypeScript

- TypeScript SDK relies on gRPC client error propagation and standard exception handling. No custom error types observed.

## 6. Design Patterns

### 6.1 CLI Architecture (Rust)

- **Argument parsing**: `clap` with derive macros for type-safe CLI argument definitions. Subcommands modeled as enums with `#[derive(Subcommand)]`.
- **Configuration layering**: Three-tier config resolution -- user-level (`~/.vorpal/settings.json`), project-level (`Vorpal.toml`), and built-in defaults. CLI flags override all layers.
- **Async runtime**: `tokio` with multi-threaded runtime (`#[tokio::main]`).
- **Logging**: `tracing` + `tracing-subscriber` with configurable log levels. Logs go to stderr; artifact output goes to stdout.

### 6.2 SDK Architecture

- **Multi-language parity**: Rust, Go, and TypeScript SDKs expose equivalent functionality. CI validates cross-SDK consistency by comparing artifact digests across all three implementations.
- **gRPC-first**: All inter-service communication uses gRPC with protobuf-generated clients/servers. Proto definitions are canonical and live in `sdk/rust/api/`.
- **Builder pattern**: Artifact construction uses a builder-style pattern with `add_artifact()`, `fetch_artifact()`, and method chaining through the `ConfigContext` struct.

### 6.3 Code Generation

- Protobuf code generation for Go and TypeScript via `protoc` with language-specific plugins (`protoc-gen-go`, `protoc-gen-go-grpc`, `protoc-gen-ts_proto`).
- Rust protobuf generation handled at build time via `tonic-prost-build` in `build.rs`.

## 7. Dependency Management

### 7.1 Rust

- **Workspace**: Three crates (`cli`, `config`, `sdk/rust`) managed as a Cargo workspace with `resolver = "2"`.
- **Version pinning**: All dependencies use exact version pins (e.g., `anyhow = { version = "1.0.100" }`) rather than semver ranges.
- **Vendoring**: Dependencies are vendored via `cargo vendor --versioned-dirs` for reproducible offline builds in CI.
- **Dev dependencies**: Minimal -- only `tempfile` in the CLI crate.

### 7.2 Go

- Standard Go modules with `go.mod`/`go.sum`. Dependencies are minimal (TOML parser, UUID, gRPC, protobuf).

### 7.3 TypeScript

- Uses Bun as the runtime and build tool. Published to npm as `@altf4llc/vorpal-sdk`.
- Dependencies: `@bufbuild/protobuf`, `@grpc/grpc-js`, `smol-toml`.
- Dev dependencies: `ts-proto` (code gen), `tsx` (validation scripts), `typescript`, `@types/bun`.

### 7.4 Automated Updates

- **Renovate** is configured (`.github/renovate.json`) with granular automerge rules:
  - GitHub Actions minor/patch: automerge.
  - Rust/Go/TypeScript production deps: automerge for patch (with 3-day minimum release age), minor automerge only for stable (>= 1.0) packages.
  - Dev dependencies: more aggressive automerge (patch always, minor for stable).
  - Docker images: patch/minor automerge with 3-day delay.
  - Terraform and Go indirect deps: manual review required.
  - Semantic commit type enforced: `chore`.
  - Lock file maintenance: weekly, automerged.

## 8. Editor and Developer Experience

- **No `.editorconfig`**: No project-wide editor configuration exists.
- **No pre-commit hooks**: No git hooks or pre-commit framework configured.
- **`rust-analyzer`**: Included in the toolchain components, enabling IDE support for Rust development.
- **Development scripts**: `script/dev.sh` bootstraps the development environment (installs rustup, protoc, and platform-specific dependencies). Platform-specific scripts exist for Debian/Ubuntu and Arch Linux.

## 9. Identified Gaps

| Gap | Impact | Severity |
|-----|--------|----------|
| No Go linting/formatting in CI | Go SDK code style is unenforced | Medium |
| No TypeScript linting in CI | Only `tsc` type checking, no style enforcement | Low |
| No Protobuf linting (e.g., `buf lint`) | Proto style consistency is manual | Low |
| No `.editorconfig` | Inconsistent editor settings across contributors | Low |
| No pre-commit hooks | Format/lint issues caught only in CI, not locally | Low |
| `#[allow(dead_code)]` in auth module | Indicates incomplete implementation or tech debt | Low |
| No Rust `#![deny(warnings)]` at crate level | Relies solely on CI clippy invocation | Low |
| Exact version pinning in Cargo.toml | Renovate handles updates, but prevents compatible version resolution across workspace | Informational |
