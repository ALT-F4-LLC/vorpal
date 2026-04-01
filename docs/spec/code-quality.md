---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "Code quality tooling, conventions, error handling patterns, and style enforcement across all languages"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - testing.md
---

# Code Quality

This document describes the code quality tooling, naming conventions, error handling patterns, and enforcement mechanisms that exist in the Vorpal project today.

Vorpal is a polyglot project with three primary languages: Rust (CLI and SDK core), Go (SDK), and TypeScript (SDK). Each language follows its ecosystem's idiomatic conventions. Code quality enforcement is tooling-based rather than documentation-based -- there are no written style guides, but the Makefile and language-specific configs enforce standards at build time.

## Language Distribution

| Language   | Role                           | Source Location         |
|------------|--------------------------------|-------------------------|
| Rust       | CLI binary, SDK core, server   | `cli/`, `sdk/rust/`, `config/` |
| Go         | SDK (alternative)              | `sdk/go/`               |
| TypeScript | SDK (alternative)              | `sdk/typescript/`       |
| Protobuf   | API contract definitions       | `sdk/rust/api/`         |

Rust is the primary implementation language. Go and TypeScript SDKs provide alternative language bindings generated from shared protobuf definitions.

## Toolchain and Formatting

| Language   | Role                          | Toolchain Version | Package Manager |
|------------|-------------------------------|-------------------|-----------------|
| Rust       | CLI, SDK, core runtime        | 1.93.1 (pinned)   | Cargo           |
| Go         | SDK, config runner            | 1.26.0            | Go modules      |
| TypeScript | SDK, config runner            | 5.9.3             | Bun / npm       |
| Protobuf   | API contract definitions      | N/A (external)    | protoc          |

- **Toolchain**: Pinned via `rust-toolchain.toml` to channel `1.93.1` with `minimal` profile.
- **Components**: `clippy`, `rust-analyzer`, `rustfmt` are explicitly included in the toolchain.
- **Edition**: All crates use Rust 2021 edition.
- **Formatter**: `rustfmt` -- enforced via `cargo fmt --all --check` (Makefile `format` target).
- **Linter**: `clippy` -- enforced via `cargo clippy -- --deny warnings` (Makefile `lint` target).
- **No custom rustfmt.toml or clippy.toml**: The project relies entirely on default rustfmt and clippy configurations. No overrides or custom lint rules exist.

### 2.1 Rust

- **Version**: Go 1.26.0 (specified in `sdk/go/go.mod`).
- **No explicit linter configuration**: No `golangci-lint` config, `.golangci.yml`, or similar tooling is present. Standard `go vet` and `go fmt` are assumed but not enforced by any visible CI or Makefile target.
- **Module path**: `github.com/ALT-F4-LLC/vorpal/sdk/go`.

### 2.2 Go

- **Runtime**: Bun (test runner via `bun test`; referenced in `package.json`).
- **Compiler**: TypeScript 5.9.3 with strict mode enabled (`"strict": true` in `tsconfig.json`).
- **Target**: ES2022 with Node16 module resolution.
- **No linter**: No ESLint, Biome, or other linter configuration exists. No `.eslintrc`, `.prettierrc`, or equivalent files are present.
- **No formatter**: No Prettier or equivalent is configured.

### 2.3 TypeScript

- **No `.editorconfig`** file exists in the repository.

## Error Handling Patterns

### Rust

The project uses a dual approach to error handling:

1. **`anyhow` for application-level errors**: The CLI and SDK pervasively use `anyhow::Result`, `anyhow!()`, and `bail!()` for error propagation. This is the dominant pattern -- nearly all functions return `anyhow::Result<T>`. Context is added via `.with_context(|| ...)` in some places (e.g., `context.rs:583`).

2. **`thiserror` for typed errors**: Used sparingly in two files (`cli/src/command/start/registry.rs` and `cli/src/command/start/auth.rs`) for domain-specific error enums that need to be matched on. Example: `BackendError` enum in `registry.rs`.

3. **`expect()` / `unwrap()` usage**: Present across the CLI codebase (88 occurrences across 13 files in `cli/src/`). Most `expect()` calls are on initialization-time operations (crypto provider setup, address parsing) where failure is considered unrecoverable. Some `unwrap()` calls exist after `is_none()` checks rather than using idiomatic `if let` or `match` patterns (e.g., `context.rs:200-201`, `context.rs:397-398`).

4. **gRPC error mapping**: Server-side code maps internal errors to `tonic::Status` codes. Client-side code matches on `tonic::Code::NotFound` for expected missing-resource cases and converts other statuses to `anyhow` errors via `bail!`.

### Go

- Standard Go error handling with `error` return values and `fmt.Errorf`.
- No custom error types observed -- uses plain error strings.
- `log.Fatalf` for unrecoverable errors in configuration loading.

### TypeScript

- TypeScript SDK is primarily generated protobuf bindings and thin wrappers. Error handling delegates to gRPC client error propagation.

### 2.4 Protobuf

- Proto files use `proto3` syntax with package namespacing (`vorpal.agent`, `vorpal.artifact`, etc.).
- `go_package` option is set on all proto files for Go code generation.
- No proto linter (e.g., `buf lint`) is configured.

- **Crate names**: Hyphenated (`vorpal-cli`, `vorpal-sdk`, `vorpal-config`).
- **Module organization**: File-per-module pattern. Subcommands map 1:1 to module files under `cli/src/command/`. SDK artifacts each get their own module file under `sdk/rust/src/artifact/`.
- **Struct naming**: PascalCase, descriptive. Config-related structs prefixed with `Vorpal` (e.g., `VorpalConfig`, `VorpalCredentials`). Context structs prefixed with `Config` (e.g., `ConfigContext`, `ConfigContextStore`).
- **Function naming**: snake_case. Getter functions use `get_` prefix consistently (e.g., `get_system`, `get_artifact_store`, `get_root_dir_path`).
- **Constants**: SCREAMING_SNAKE_CASE (e.g., `DEFAULT_NAMESPACE`, `DEFAULT_TAG`, `DEFAULT_GRPC_CHUNK_SIZE`).

### Go

- Standard Go conventions: PascalCase for exported identifiers, camelCase for unexported.
- Struct fields use camelCase for unexported fields (e.g., `artifactInputCache`).
- Package names are lowercase single words (`config`, `artifact`, `store`).

### TypeScript

- ESM module system (`"type": "module"` in `package.json`).
- Package scoped under `@altf4llc/vorpal-sdk`.

## Code Organization Patterns

### Module Structure

The Rust codebase follows a consistent flat-module pattern:

- **CLI subcommands**: Each subcommand (`build`, `init`, `inspect`, `run`, `start`, `system`) is a separate module in `cli/src/command/`. Nested subcommands use subdirectories (e.g., `start/agent.rs`, `start/worker.rs`, `start/registry.rs`).
- **SDK artifacts**: Each tool/artifact type gets its own module file (`bun.rs`, `cargo.rs`, `go.rs`, `nodejs.rs`, etc.) -- 30+ artifact modules under `sdk/rust/src/artifact/`.
- **API layer**: Generated from protobuf via `tonic-prost-build` in `build.rs`. Proto files live in `sdk/rust/api/` organized by service domain (`agent/`, `archive/`, `artifact/`, `context/`, `worker/`).

### Cross-Language API Consistency

All three SDKs share the same protobuf definitions from `sdk/rust/api/`:

- **Rust**: Generated at build time via `tonic-prost-build` (`build.rs`).
- **Go**: Generated via `protoc` with `protoc-gen-go` and `protoc-gen-go-grpc` (Makefile `generate` target).
- **TypeScript**: Generated via `protoc` with `ts-proto` plugin (Makefile `generate` target, also `npm run generate:proto`).

The Go SDK mirrors the Rust SDK's context/config architecture (e.g., `ConfigContext` struct, `ParseArtifactAlias` function with identical behavior documented in Rust comments: "This mirrors the Go implementation").

## Design Patterns in Use

### Builder/Configuration Pattern

The CLI uses a layered configuration resolution pattern:

1. Built-in defaults (hardcoded in `VorpalConfig::defaults()`).
2. User-level config (`~/.vorpal/settings.json`).
3. Project-level config (`Vorpal.toml`).
4. CLI flags (highest precedence).

Resolution happens at startup via `config::resolve_config()` with graceful fallback to defaults on parse failure.

### gRPC Service Pattern

Server-side services implement tonic-generated traits (e.g., `ContextService`, `ArchiveService`, `ArtifactService`). A backend trait pattern is used for storage abstraction:

- `ArchiveBackend` trait with `LocalBackend` and `S3Backend` implementations.
- Backend selection at runtime via CLI flag (`--registry-backend local|s3`).

### Structured Logging

- **Rust**: `tracing` crate with `tracing-subscriber`. Logs go to stderr. Debug/trace levels enable file and line number output. Log level is configurable via `--level` CLI flag. Used across all 13 source files in the CLI.
- **Go**: Standard `log` package. No structured logging framework.
- **TypeScript**: No logging framework observed.

## Dependency Management

### Rust

- Workspace-level `Cargo.toml` with three members: `cli`, `config`, `sdk/rust`.
- Dependencies are pinned to exact versions (no `^` or `~` ranges) in all `Cargo.toml` files.
- `Cargo.lock` is committed (appropriate for a binary project).
- Offline builds supported via `cargo vendor` (Makefile `vendor` target, `.cargo/config.toml` for vendored sources).
- `cargo-machete` metadata in SDK's `Cargo.toml` for unused dependency detection.

The GitHub Actions workflow (`.github/workflows/vorpal.yaml`) enforces code quality through a dedicated `code-quality` job that gates the `build` job:

- `go.mod` with pinned versions. Minimal dependency set: `toml`, `uuid`, `grpc`, `protobuf`.

The `code-quality` job runs on `macos-latest` only and executes:

- `package.json` with exact version pins (no `^` or `~`). Minimal runtime deps: `@bufbuild/protobuf`, `@grpc/grpc-js`, `smol-toml`.

## Build System

The `Makefile` serves as the single entry point for development workflows:

| Target     | Purpose                              |
|------------|--------------------------------------|
| `build`    | `cargo build` (default target)       |
| `check`    | `cargo check`                        |
| `format`   | `cargo fmt --all --check`            |
| `lint`     | `cargo clippy -- --deny warnings`    |
| `test`     | `cargo test`                         |
| `clean`    | Clean build artifacts and vendor dir |
| `vendor`   | Vendor Rust dependencies             |
| `generate` | Regenerate Go and TS protobuf code   |
| `dist`     | Package release tarball              |

The Makefile supports both debug and release builds via `TARGET` variable, and offline builds via vendored sources.

## 9. Identified Gaps

### No CI Configuration in Repository

No GitHub Actions workflows, Jenkinsfile, or other CI configuration files are present in the repository. The Makefile targets (`format`, `lint`, `test`) exist but there is no visible automation running them on pull requests.

### Go and TypeScript Quality Tooling

- **Go**: No linter (golangci-lint), no formatter enforcement, no Makefile targets for Go code quality.
- **TypeScript**: No linter (ESLint/Biome), no formatter (Prettier), no Makefile targets for TypeScript code quality. The `tsconfig.json` has `strict: true` which provides type checking, but no additional static analysis.

### Inconsistent Error Handling

- The Rust codebase mixes `unwrap()` after `is_none()` checks with more idiomatic patterns. Some places could use `if let` or `?` operator instead.
- `#[allow(clippy::too_many_arguments)]` appears in `context.rs`, indicating some functions have grown beyond typical parameter limits.
- `#[allow(dead_code)]` on several `Claims` struct fields suggests the auth module has fields that exist for future use.

### No Editor Configuration

No `.editorconfig`, no workspace-level VS Code settings, no shared formatter configurations. Developers must rely on the toolchain-installed `rustfmt` and `clippy` components for Rust, with nothing enforcing consistency for Go or TypeScript.

### Limited Code Documentation

- Rust doc comments (`///`) are used sparingly -- primarily on the `parse_artifact_alias` function and `ArtifactAlias` struct in `context.rs`. Most public APIs in the SDK lack doc comments.
- No `rustdoc` generation target in the Makefile.
- Go code has no godoc comments on exported types.

### No Pre-commit Hooks

No `.pre-commit-config.yaml`, `husky`, or `lefthook` configuration exists. Format and lint checks are available via Makefile but not enforced before commits.
