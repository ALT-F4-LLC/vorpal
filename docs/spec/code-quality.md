---
project: "vorpal"
maturity: experimental
last_updated: "2026-03-20"
updated_by: "@staff-engineer"
scope: "Linting, formatting, naming conventions, error handling, and code style"
owner: "@staff-engineer"
dependencies:
  - architecture.md
---

# Code Quality

## Overview

Vorpal enforces code quality through Rust-native tooling (clippy, rustfmt) integrated into CI, with warnings-as-errors policy. The Go and TypeScript SDKs have lighter quality gates. The project follows Rust idiomatic patterns with consistent error handling via `anyhow` and `thiserror`.

## Linting and Formatting

### Rust

- **Formatter**: `cargo fmt --all --check` -- enforced in CI (code-quality job)
- **Linter**: `cargo clippy -- --deny warnings` -- all warnings are errors in CI
- **Toolchain**: Pinned via `rust-toolchain.toml` to channel `1.93.1` with `clippy`, `rustfmt`, and `rust-analyzer` components
- **No custom clippy configuration**: Uses default clippy lints with no `#![allow(...)]` or `clippy.toml` overrides (aside from individual `#[allow(dead_code)]` on specific fields)

### Go

- No linter configuration found in the repository (no `.golangci.yml`, no `staticcheck` config)
- `staticcheck` binary exists as an artifact builder in the Rust SDK (`sdk/rust/src/artifact/staticcheck.rs`) but is not run in CI
- `goimports` artifact builder exists but is not integrated into CI quality gates
- Go tests exist in `sdk/go/pkg/config/` (`context_test.go`, `context_auth_test.go`)

### TypeScript

- No ESLint or Prettier configuration
- TypeScript strict mode via `tsc` (used as build step)
- `bun test` configured in `package.json` but no test files observed
- No dedicated linting step in CI for the TypeScript SDK

### Editor Configuration

- No `.editorconfig` file
- No VS Code settings or recommended extensions
- `rust-analyzer` included in the toolchain components

## Naming Conventions

### Rust

- **Crates**: `vorpal-cli`, `vorpal-config`, `vorpal-sdk` -- kebab-case
- **Modules**: snake_case files (`build.rs`, `config_cmd.rs`)
- **Functions**: snake_case (`get_default_namespace`, `build_source`)
- **Types**: PascalCase (`AgentServer`, `RunArgs`, `SourceCacheKey`)
- **Constants**: SCREAMING_SNAKE_CASE (`DEFAULT_CHUNKS_SIZE`, `DUPLEX_BUF_SIZE`)
- **Enum variants**: PascalCase (`ServerBackend::Local`, `ArtifactSourceType::Http`)

### Go

- Standard Go conventions: PascalCase for exports, camelCase for unexported
- Package names: lowercase single words (`artifact`, `config`, `store`)

### TypeScript

- PascalCase for classes and types (`ConfigContext`, `ArtifactSystem`)
- camelCase for functions and variables
- Files: kebab-case or camelCase (`context.ts`, `system.ts`)

### Protobuf

- Package: `vorpal.<service>` (e.g., `vorpal.artifact`)
- Messages: PascalCase (`ArtifactSource`, `PrepareArtifactRequest`)
- Fields: snake_case (`artifact_digest`, `source_digest`)
- Enums: SCREAMING_SNAKE_CASE values (`AARCH64_DARWIN`, `UNKNOWN_SYSTEM`)
- Services: PascalCase with `Service` suffix (`AgentService`, `ArtifactService`)

## Error Handling

### Patterns

- **`anyhow::Result`**: Used throughout the CLI and SDK for error propagation with context. All top-level functions return `anyhow::Result<()>`.
- **`thiserror`**: Used for structured error types where pattern matching is needed (e.g., `AuthError` in `auth.rs`).
- **`tonic::Status`**: gRPC service methods convert internal errors to `Status` with appropriate codes (`InvalidArgument`, `Internal`, `NotFound`, `Unauthenticated`, `PermissionDenied`, `FailedPrecondition`).
- **`bail!` macro**: Used for early returns with error messages, especially for validation.
- **`.map_err()`**: Consistent use of `map_err` to add context to underlying errors (e.g., "failed to bind main server on {}: {}").

### Error Messages

- Prefixed with service context in logs (e.g., `agent |>`, `auth |>`)
- User-facing errors include actionable guidance (e.g., "run 'vorpal system keys generate' or copy from agent")
- Exit code 1 on fatal errors via `process::exit(1)`

### Logging

- `tracing` crate with `FmtSubscriber`
- Configurable level via `--level` flag (default: `INFO`)
- Debug/trace modes include file and line numbers
- All log output goes to stderr (stdout reserved for artifact output)

## Design Patterns

### Builder Pattern

Extensively used across all three SDKs for artifact construction:
- `Rust::new("name", systems).with_bins(...).with_includes(...).build(ctx)`
- `language.NewGo("name", systems).WithBuildDirectory(...).WithIncludes(...).Build(ctx)`
- `new TypeScript("name", systems).withEntrypoint(...).withIncludes(...).build(context)`

### Service Pattern

gRPC services follow a consistent pattern:
1. Struct implementing the service trait (`AgentServer`, `ArchiveServer`, etc.)
2. `new()` constructor
3. `#[tonic::async_trait]` implementation of the generated service trait
4. Request validation at the top of each handler

### Layered Configuration

Configuration resolution follows a layered approach:
1. Built-in defaults
2. User-level config (`~/.vorpal/settings.json`)
3. Project-level config (`Vorpal.toml`)
4. CLI flags (highest priority)

Explicit CLI flags always override config values; config values override defaults.

## Module Organization

### CLI Module Structure

```
command/
├── build.rs          # Build orchestration
├── config.rs         # Configuration types and resolution
├── config_cmd.rs     # Config subcommand handlers
├── init.rs           # Project initialization
├── inspect.rs        # Artifact inspection
├── lock.rs           # Lockfile management
├── run.rs            # Artifact execution
├── start.rs          # Service startup
├── start/            # Service implementations
│   ├── agent.rs      # Agent gRPC service
│   ├── auth.rs       # OIDC validation
│   ├── registry/     # Registry services
│   └── worker.rs     # Worker service
├── store/            # Local storage utilities
│   ├── archives.rs   # Compression/decompression
│   ├── hashes.rs     # Digest computation
│   ├── notary.rs     # Secret encryption
│   ├── paths.rs      # Path management
│   └── temps.rs      # Temporary file management
├── system/           # System management
└── template/         # Project templates
```

## Gaps and Areas for Improvement

- No Go linter integration in CI (staticcheck, golangci-lint)
- No TypeScript linter or formatter in CI
- No `.editorconfig` for cross-editor consistency
- No pre-commit hooks
- No dead code analysis beyond `#[allow(dead_code)]` annotations
- No dependency audit tool integration (e.g., `cargo audit`, `npm audit`)
- No code coverage tooling or thresholds
- `#[allow(dead_code)]` on several fields in `Claims` struct suggests API surface is wider than current usage
