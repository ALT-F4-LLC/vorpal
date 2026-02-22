# Code Quality Specification

This document describes the coding standards, naming conventions, error handling patterns,
design patterns, and project-specific style decisions that exist in the Vorpal codebase today.

---

## 1. Languages and Runtimes

The project is polyglot, with three primary implementation languages:

| Language   | Role                        | Location(s)            | Toolchain                       |
|------------|-----------------------------|------------------------|---------------------------------|
| Rust       | CLI, SDK, config binaries   | `cli/`, `config/`, `sdk/rust/` | Rust 1.89.0 (pinned via `rust-toolchain.toml`) |
| TypeScript | SDK (new)                   | `sdk/typescript/`      | TypeScript 5.9.3, Bun (tests), Node 16 module target |
| Go         | SDK (separate repo tree)    | `sdk/go/` (not in this worktree but referenced) | Standard Go toolchain |
| Protobuf   | API contract definitions    | `sdk/rust/api/`        | `protoc` with `tonic-prost-build` (Rust), `ts-proto` (TS), `protoc-gen-go` (Go) |

Shell scripts (Bash) are used in build steps, CI, and operational tooling under `script/`.

---

## 2. Formatter and Linter Configuration

### 2.1 Rust

- **Formatter**: `rustfmt` (included in the pinned toolchain via `components = ["rustfmt"]`).
  No custom `rustfmt.toml` or `.rustfmt.toml` exists -- the project uses default rustfmt settings.
- **Linter**: `clippy` (included in the pinned toolchain via `components = ["clippy"]`).
  No custom `clippy.toml` exists. Clippy is invoked with `--deny warnings` in CI
  (`make lint` target: `cargo clippy $(CARGO_FLAGS) -- --deny warnings`).
- **CI enforcement**: The `code-quality` job in `.github/workflows/vorpal.yaml` runs
  `make format` (rustfmt check mode) and `make TARGET=release lint` (clippy deny warnings).
  Code cannot merge with format or lint violations.
- **Notable clippy allowances**: `#[allow(clippy::too_many_arguments)]` is used on a few
  functions with large parameter lists (e.g., `ConfigContext::new`, `config::start`).

### 2.2 TypeScript

- **Formatter**: None configured. No `.prettierrc`, `.editorconfig`, or ESLint config exists
  at the project level for TypeScript code.
- **Linter**: None configured. No ESLint or Biome config exists for the TypeScript SDK.
- **TypeScript compiler**: `tsconfig.json` with `"strict": true`, `"forceConsistentCasingInFileNames": true`.
  Target is `ES2022` with `Node16` module resolution.
- **Gap**: The TypeScript SDK has no automated formatting or linting enforcement in CI.
  Only `tsc` (type checking via `bun build`) and `bun test` are run.

### 2.3 Shell Scripts

- No shellcheck or shell linting configuration exists.
- Shell scripts in `script/` use `#!/bin/bash` or `#!/usr/bin/env bash`.
- Build step scripts consistently use `set -euo pipefail` for strict error handling.

### 2.4 Protobuf

- No `buf.yaml` or protobuf linting configuration exists.
- Proto files follow standard proto3 conventions with `go_package` options.

---

## 3. Naming Conventions

### 3.1 Rust

- **Crate names**: `vorpal-cli`, `vorpal-sdk`, `vorpal-config` (kebab-case, prefixed with `vorpal-`).
- **Module names**: `snake_case` (e.g., `linux_vorpal`, `protoc_gen_go`, `rust_toolchain`).
- **Struct names**: `PascalCase` (e.g., `ConfigContext`, `ArtifactSource`, `RunArgsArtifact`).
- **Function names**: `snake_case` (e.g., `get_default_address`, `build_channel`, `parse_artifact_alias`).
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_NAMESPACE`, `DEFAULT_TAG`).
- **Enum variants**: `PascalCase` (e.g., `CommandSystem::Keys`, `SettingsSource::Default`).
- **Type aliases for generated code**: Proto-generated types are accessed through a nested
  `api` module (`api::artifact::Artifact`, `api::agent::AgentServiceClient`).
- **Builder pattern structs**: Named after the concept they build (e.g., `Artifact`, `ArtifactStep`,
  `TypeScript`, `Go`, `Rust`) with `with_*` methods for optional configuration and a terminal
  `build()` method.

### 3.2 TypeScript

- **File names**: `camelCase` or `kebab-case` for source files (e.g., `artifact.ts`, `context.ts`,
  `step.ts`). Test files use `kebab-case` with `.test.ts` suffix in `__tests__/` directory.
- **Class names**: `PascalCase` with `Builder` suffix (e.g., `ArtifactBuilder`,
  `ArtifactSourceBuilder`, `JobBuilder`, `RustBuilder`, `TypeScriptBuilder`).
- **Function names**: `camelCase` (e.g., `getEnvKey`, `parseArtifactAlias`, `getSystemDefaultStr`).
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `DEFAULT_NAMESPACE`, `DEFAULT_TAG`,
  `VORPAL_ROOT_DIR`).
- **Private members**: Prefixed with underscore (e.g., `_name`, `_artifacts`, `_store`).
- **Type imports**: Use `import type` for type-only imports, separating runtime from type imports.

### 3.3 Protobuf

- **Package names**: Dot-separated namespace (e.g., `vorpal.artifact`, `vorpal.agent`).
- **Service names**: `PascalCase` with `Service` suffix (e.g., `ArtifactService`, `AgentService`).
- **Message names**: `PascalCase` (e.g., `ArtifactSource`, `ArtifactStep`, `PrepareArtifactRequest`).
- **Enum names**: `PascalCase` (e.g., `ArtifactSystem`).
- **Enum values**: `SCREAMING_SNAKE_CASE` (e.g., `AARCH64_DARWIN`, `UNKNOWN_SYSTEM`).
- **Field names**: `snake_case` (e.g., `artifact_namespace`, `artifact_unlock`).

### 3.4 Configuration Files

- **Project config**: `Vorpal.toml` (PascalCase prefix). Language-specific variants use a
  dot-suffix: `Vorpal.go.toml`, `Vorpal.ts.toml`.
- **Lock file**: `Vorpal.lock` (PascalCase prefix).
- **TOML keys**: `snake_case` (e.g., `bun_version`, `source.typescript.entrypoint`).

---

## 4. Error Handling Patterns

### 4.1 Rust

The project uses `anyhow` for error handling throughout, with a consistent pattern:

- **Result type**: `anyhow::Result<T>` is the standard return type for fallible functions.
  No custom error types are defined; `anyhow::Error` is used exclusively.
- **Error propagation**: The `?` operator is used for propagation. Context is added via
  `.with_context(|| format!("..."))` or `.map_err(|e| anyhow!("..."))` at significant
  boundaries.
- **Validation errors**: `bail!()` is used for input validation failures (e.g., empty names,
  unsupported systems, missing files). Error messages are descriptive and often multi-line
  with remediation hints.
- **Fatal errors**: `exit(1)` is used in a few places in the CLI after logging an error via
  `tracing::error!()`. This pattern appears in stream processing loops where recovery is
  not possible.
- **`.expect()` usage**: Used for conditions that should never fail (e.g., serialization,
  regex compilation, channel creation). Messages follow `"failed to ..."` format.
- **gRPC error handling**: gRPC status codes are matched explicitly (e.g., `Code::NotFound`,
  `Code::Unavailable`) with different handling per code. Non-NotFound errors are generally
  treated as fatal.

### 4.2 TypeScript

- **Error type**: Standard `Error` with descriptive messages. No custom error classes.
- **Error messages**: Multi-line format with context, explanation, and remediation steps
  (matching the Rust pattern). Example:
  ```typescript
  throw new Error(
    `Agent service is unavailable (connection refused or dropped).\n\n` +
    `  Could not reach the agent at the configured address.\n\n` +
    `  To fix this:\n` +
    `    1. Make sure the Vorpal agent is running:\n` +
    `         vorpal system services start\n`
  );
  ```
- **gRPC error handling**: Mirrors the Rust approach -- explicit matching on `grpc.status`
  codes (`NOT_FOUND`, `UNAVAILABLE`, `DEADLINE_EXCEEDED`) with specific error messages per code.
- **Validation**: Guard clauses with `throw new Error(...)` at the top of functions for
  precondition checks (e.g., empty name, empty steps, system not in supported list).

### 4.3 Shell Scripts

- Build step scripts use `set -euo pipefail` consistently.
- No explicit error handling beyond the bash strict mode.

---

## 5. Design Patterns

### 5.1 Builder Pattern

The builder pattern is the dominant design pattern in both Rust and TypeScript SDKs.
All artifact types use this pattern:

**Rust**: Struct with `new()` constructor, `with_*()` chainable methods, and `async build()`
terminal method that takes a `&mut ConfigContext`:

```rust
TypeScript::new(&config.name, vec![config_system])
    .with_includes(includes)
    .with_bun_version(bun_version)
    .with_entrypoint(entrypoint)
    .build(&mut config_context)
    .await?
```

**TypeScript**: Class with constructor, `with*()` chainable methods returning `this`, and
`async build()` terminal method that takes a `ConfigContext`:

```typescript
new RustBuilder("vorpal", SYSTEMS)
    .withBins(["vorpal"])
    .withIncludes(["cli", "sdk/rust"])
    .withPackages(["vorpal-cli", "vorpal-sdk"])
    .build(context);
```

Both SDKs implement the same set of builders: `Artifact`, `ArtifactSource`, `ArtifactStep`,
`Job`, `Process`, `ProjectEnvironment`, `UserEnvironment`, `Argument`, plus language-specific
builders (`Rust`, `Go`, `TypeScript`).

### 5.2 Cross-SDK Parity

A critical design constraint: all three SDKs (Rust, Go, TypeScript) must produce
**byte-identical JSON serialization** for the same artifact definition. This is because
artifact digests (SHA-256 of the JSON-serialized artifact) serve as content-addressable
identifiers throughout the system.

This manifests as:
- Custom JSON serialization in TypeScript (`serializeArtifact()`) that matches Rust's
  `serde_json::to_vec` output field-for-field (snake_case keys, proto field order, all
  fields always present, enums as integers).
- Deterministic sorting of secrets, symlinks, and other collections before building.
- Parity test suites that verify identical digests across SDKs.
- Comments like `// CRITICAL: Shell script template must be character-for-character identical`
  appear throughout the TypeScript SDK.

### 5.3 gRPC Client-Server Architecture

- Protobuf definitions serve as the single source of truth for all API contracts.
- Proto files live in `sdk/rust/api/` and are compiled into each SDK via language-specific
  code generators.
- The Rust build uses `tonic-prost-build` in `build.rs` to generate code at compile time.
- The TypeScript SDK uses `ts-proto` with specific options (`snakeToCamel=false`,
  `forceLong=number`, `outputServices=grpc-js`).

### 5.4 Module Organization

**Rust crates:**
- `vorpal-cli` (`cli/`): The main CLI binary. Organized as `command/` submodules corresponding
  to CLI subcommands (`build`, `init`, `start`, `system`, `store`, etc.).
- `vorpal-sdk` (`sdk/rust/`): The SDK library. Public API exposed through `lib.rs` with
  `api` (generated), `artifact` (builders), `cli` (argument parsing), and `context` (runtime)
  modules.
- `vorpal-config` (`config/`): The project's own Vorpal configuration binary (dogfooding).
  Organized around artifact types.

**TypeScript SDK:**
- `src/` root: Core modules (`artifact.ts`, `context.ts`, `cli.ts`, `system.ts`, `index.ts`).
- `src/api/`: Generated protobuf code (gitignored, generated from `ts-proto`).
- `src/artifact/`: Sub-builders (`step.ts`, `language/rust.ts`, `language/typescript.ts`).
- `src/__tests__/`: Test files using Bun test runner.
- `dist/`: Compiled output (gitignored).

### 5.5 Config Resolution Pattern

The CLI uses a three-layer config resolution: built-in defaults < user config
(`~/.vorpal/settings.json`) < project config (`Vorpal.toml`). CLI flags override all.
The `ResolvedSettings` struct tracks provenance (`SettingsSource::Default`, `User`, `Project`)
for debugging.

---

## 6. Code Style Conventions

### 6.1 Rust Style

- **Imports**: Grouped with `use` statements at the top of each file. External crate imports
  first, then internal crate imports (`use crate::...`), then std library. No explicit
  ordering enforcement beyond rustfmt defaults.
- **Blank lines**: Used to separate logical sections within functions. Section comments like
  `// Setup step` or `// Build artifact` are common.
- **String formatting**: Uses `format!()` and `formatdoc!{}` (from `indoc` crate) for
  multi-line strings. String interpolation via format macros is preferred over concatenation.
- **Comments**: Sparse. Used primarily for:
  - `TODO:` markers for known future work
  - Section headers within long functions
  - Doc comments on public API items (especially in the SDK)
  - Inline explanations for non-obvious behavior
- **Function signatures**: Long signatures use multi-line formatting. `#[allow(clippy::too_many_arguments)]`
  is used rather than introducing parameter objects in most cases.
- **Async**: `tokio` is the async runtime. `#[tokio::main]` on entry points.
  `Box::pin()` is used for recursive async calls.

### 6.2 TypeScript Style

- **Imports**: Use `.js` extension in import paths (required for Node16 module resolution
  with ESM). Type-only imports use `import type { ... }`.
- **Class members**: Private members use `private` keyword and underscore prefix (`private _name`).
- **JSDoc comments**: Used on public API items with `@param`, `@returns`, `@throws`, and
  `@example` tags. Comments reference Rust source locations (e.g., `Matches Rust
  sdk/rust/src/artifact.rs Artifact impl (lines 211-256)`).
- **Section separators**: Horizontal rule comments (`// ---------------------------------------------------------------------------`)
  used to visually separate major sections within files.
- **Template literals**: Used for string interpolation and multi-line strings.
- **Promise handling**: `new Promise<T>((resolve, reject) => ...)` wrappers around callback-based
  gRPC APIs. Async/await for control flow.
- **Module exports**: Centralized through `index.ts` barrel file with explicit re-exports.
  Both value and type exports are included.

### 6.3 General Conventions

- **No docstrings on private/internal code** unless the behavior is non-obvious.
- **Descriptive error messages** with multi-line format including context, diagnosis, and
  fix suggestions. This is a deliberate project-wide pattern in both Rust and TypeScript.
- **Deterministic output**: Collections are sorted before serialization throughout (secrets
  by name, symlinks by source, artifact digests for listing).
- **No magic numbers**: Version strings, default paths, and constants are named.

---

## 7. Dependency Management

### 7.1 Rust

- **Workspace**: Cargo workspace with three members (`cli`, `config`, `sdk/rust`).
  Resolver version 2.
- **Version pinning**: Dependencies use exact versions (e.g., `anyhow = { version = "1.0.100" }`),
  not semver ranges.
- **Vendoring**: `cargo vendor --versioned-dirs` support via `make vendor` and `.cargo/config.toml`.
  Offline builds supported via `--offline` flag in release mode.
- **Dependency updates**: Renovate bot configured (`.github/renovate.json`) with weekly
  lock file maintenance and semantic commit type `chore`.
- **Toolchain pinning**: `rust-toolchain.toml` pins to Rust 1.89.0 with `profile = "minimal"`
  and explicit components (`clippy`, `rust-analyzer`, `rustfmt`).

### 7.2 TypeScript

- **Package manager**: Bun (`bun.lock` lockfile format).
- **Runtime dependencies**: `@bufbuild/protobuf`, `@grpc/grpc-js`, `smol-toml`.
- **Dev dependencies**: `@types/bun`, `ts-proto`, `tsx`, `typescript`.
- **Module system**: ESM (`"type": "module"` in `package.json`).
- **Publishing**: Scoped package `@vorpal/sdk` with `"access": "public"`.

---

## 8. CI Quality Gates

The CI pipeline (`.github/workflows/vorpal.yaml`) enforces these quality gates:

1. **vendor**: Dependency vendoring and `cargo check` (compilation check).
2. **code-quality**: `cargo fmt --all --check` (format check) and
   `cargo clippy -- --deny warnings` (lint check). Runs on macOS only.
3. **build**: `cargo build`, `cargo test`, and `cargo dist` (packaging).
4. **test**: End-to-end integration tests that build artifacts using all three SDKs
   (Rust, Go, TypeScript) and verify cross-SDK digest parity.

There is no separate CI job for TypeScript formatting, linting, or type checking.
TypeScript quality is indirectly validated through the build process (which uses `tsc`)
and `bun test` (which runs the SDK test suite).

---

## 9. Known Gaps and Improvement Opportunities

| Area | Gap | Impact |
|------|-----|--------|
| TypeScript linting | No ESLint/Biome configuration | Inconsistent TS code style possible |
| TypeScript formatting | No Prettier/dprint configuration | No automated format enforcement |
| TypeScript CI | No dedicated TS quality job | TS issues caught late or not at all |
| Shell linting | No shellcheck integration | Shell script bugs not caught statically |
| Protobuf linting | No buf.yaml or protolint | Proto style not enforced |
| Editor config | No `.editorconfig` file | Inconsistent indentation/line endings across editors |
| Clippy config | No `clippy.toml` customization | Using all defaults, no project-specific tuning |
| Rustfmt config | No `rustfmt.toml` customization | Using all defaults (acceptable but undocumented) |
| Custom error types | `anyhow::Error` used everywhere | All errors are opaque strings; no structured error matching possible for callers |
| Code duplication | Build step logic repeated between Rust and TS SDKs | Must be kept in sync manually; parity tests mitigate but don't prevent drift |

---

## 10. Logging and Observability

- **Rust**: Uses `tracing` crate with `tracing-subscriber` for structured logging.
  Log level is configurable via `--level` CLI flag (default: `INFO`). Debug/trace levels
  include file and line numbers. Logs go to stderr.
- **TypeScript**: Uses `console.log` and `console.error` directly. No structured logging
  framework.
- **Build output format**: Both SDKs use `"{artifact_name} |> {message}"` format for
  build step output, providing consistent log formatting across languages.
