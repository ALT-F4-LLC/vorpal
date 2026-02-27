# Code Quality

Project-specific coding standards, naming conventions, error handling patterns, design patterns in
use, and style decisions for the Vorpal codebase.

---

## Languages & Toolchains

Vorpal is a polyglot project with three SDK implementations that must produce **byte-identical
artifact digests** for the same inputs. This parity constraint is the single most important code
quality concern in the project.

| Language   | Version    | Role                                   |
|------------|------------|----------------------------------------|
| Rust       | 1.93.1     | CLI, services (agent/registry/worker), SDK, config binary |
| Go         | 1.24.2     | SDK, config binary                     |
| TypeScript | ES2022     | SDK (runs on Bun)                      |
| Protobuf   | proto3     | API contract definitions               |

### Rust Toolchain

Pinned via `rust-toolchain.toml`:

```toml
[toolchain]
auto_self_update = "disable"
channel = "1.93.1"
components = [ "clippy", "rust-analyzer", "rustfmt" ]
profile = "minimal"
```

Components `clippy`, `rustfmt`, and `rust-analyzer` are mandated by the toolchain file but **no
`rustfmt.toml`, `clippy.toml`, or `.clippy.conf`** files exist. The project relies on default
Clippy and rustfmt rules.

One explicit Clippy override exists:
- `#[allow(clippy::too_many_arguments)]` in `cli/src/command/config.rs:387` for the `start()`
  function.

### Go Toolchain

- Go 1.24.2 with minimal dependencies (TOML parser, UUID, gRPC, protobuf).
- **No `.golangci.yml`** or linter configuration file. Standard `go vet` and `go build` are the
  implicit quality gates.
- No `gofmt`/`goimports` configuration beyond defaults.

### TypeScript Toolchain

- TypeScript 5.9.3 with strict mode enabled.
- Bun as runtime and test runner (`bun test`).
- ESM modules (`"type": "module"` in package.json, `"module": "Node16"` in tsconfig).
- **No ESLint, Prettier, or other linter/formatter configuration.**

`tsconfig.json` enforces:
```json
{
  "strict": true,
  "forceConsistentCasingInFileNames": true,
  "esModuleInterop": true,
  "skipLibCheck": true
}
```

---

## Linter & Formatter Configuration

### What Exists

| Tool     | Config File       | Status                        |
|----------|-------------------|-------------------------------|
| Clippy   | (none)            | Included in toolchain, default rules |
| rustfmt  | (none)            | Included in toolchain, default rules |
| go vet   | (none)            | Implicit via Go toolchain     |
| TSC      | `tsconfig.json`   | Strict mode enabled           |

### What Does NOT Exist

- No `.editorconfig`
- No `.eslintrc` / `eslint.config.js`
- No `.prettierrc`
- No `rustfmt.toml`
- No `clippy.toml`
- No `.golangci.yml`
- No pre-commit hooks
- No CI lint/format check pipeline (no `.github/workflows/` directory)

**Gap:** There is no automated enforcement of formatting or linting. Quality currently depends
on developer discipline and toolchain defaults.

---

## Naming Conventions

### Rust

- **Crates:** `vorpal-cli`, `vorpal-sdk`, `vorpal-config` (hyphenated kebab-case).
- **Modules:** `snake_case` file names (`artifact.rs`, `linux_vorpal.rs`, `oci_image.rs`).
- **Functions:** `snake_case` (`get_default_address`, `get_env_key`, `get_system_default_str`).
- **Types:** `PascalCase` (`ArtifactSource`, `ArtifactStep`, `WorkerServer`).
- **Enum variants:** `PascalCase` (`Command::Build`, `SettingsSource::Project`).
- **Constants:** `SCREAMING_SNAKE_CASE` (`DEFAULT_CHUNKS_SIZE`, `DEFAULT_NAMESPACE`).
- **Builder methods:** `with_*` prefix pattern (`with_artifacts`, `with_secrets`, `with_require`).

### Go

- **Packages:** `lowercase` single-word (`artifact`, `config`, `store`).
- **Exported types:** `PascalCase` (`ConfigContext`, `ArtifactSource`, `DevelopmentEnvironment`).
- **Exported functions:** `PascalCase` with `New` prefix for constructors
  (`NewArtifact`, `NewProcess`, `NewCommand`).
- **Unexported:** `camelCase` (`parseArtifactAlias`, `fetchArtifacts`, `refreshAccessToken`).
- **Builder methods:** `With*` prefix pattern (`WithArtifacts`, `WithSecrets`, `WithRequire`).
- **Test functions:** `Test` prefix with descriptive names
  (`TestParseArtifactAlias`, `TestClientAuthHeaderValid`).

### TypeScript

- **Classes:** `PascalCase` (`Artifact`, `ArtifactSource`, `ConfigContext`).
- **Functions:** `camelCase` (`getEnvKey`, `parseCliArgs`, `getSystem`).
- **Private fields:** `_camelCase` prefix (`_name`, `_digest`, `_artifacts`).
- **Files:** `camelCase.ts` (`artifact.ts`, `context.ts`, `system.ts`).
- **Builder methods:** `with*` prefix pattern (`withArtifacts`, `withSecrets`, `withRequire`).

### Protobuf

- **Package:** `vorpal.<domain>` (`vorpal.artifact`, `vorpal.agent`).
- **Messages:** `PascalCase` (`ArtifactSource`, `ArtifactStep`).
- **Fields:** `snake_case` (`artifact_namespace`, `access_token`).
- **Enums:** `SCREAMING_SNAKE_CASE` values (`AARCH64_DARWIN`, `X8664_LINUX`).
- **Services:** `PascalCase` with `Service` suffix (`ArtifactService`, `WorkerService`).
- **RPCs:** `PascalCase` verbs (`GetArtifact`, `StoreArtifact`, `BuildArtifact`).

### Cross-Language Naming Parity

Functions that must produce identical results across SDKs share consistent naming adapted to
each language's conventions:

| Concept          | Rust                  | Go                   | TypeScript           |
|------------------|-----------------------|----------------------|----------------------|
| Env key helper   | `get_env_key()`       | `GetEnvKey()`        | `getEnvKey()`        |
| System default   | `get_system_default_str()` | `GetSystemDefaultStr()` | `getSystemDefaultStr()` |
| Default address  | `get_default_address()` | (in `NewCommand()`)  | (in `parseCliArgs()`) |
| Artifact builder | `Artifact::new()`     | `NewArtifact()`      | `new Artifact()`     |

---

## Error Handling Patterns

### Rust

- **`anyhow::Result`** used pervasively for application-level error handling in the CLI and SDK.
- **`anyhow!()` and `bail!()`** macros for ad-hoc error creation.
- **`.with_context()`** used for adding context to I/O and parsing errors (seen in config loading).
- **`thiserror`** is a dependency but not visibly used for custom error types in the current codebase.
- **gRPC errors:** Worker and service code uses `tonic::Status` with appropriate status codes
  (`Status::invalid_argument`, `Status::internal`, `Status::not_found`, `Status::already_exists`).
- **Process exits:** `std::process::exit(1)` used in CLI for fatal user-facing errors after
  logging via `tracing::error!`.
- **`.expect()`** used sparingly for truly unrecoverable initialization failures
  (`ring::default_provider().install_default().expect(...)`).

### Go

- **Standard `error` returns** — no custom error types. All errors are plain `fmt.Errorf()`.
- **Error wrapping** with `%w` verb used in most places (`fmt.Errorf("failed to ...: %w", err)`).
- **`log.Fatal` / `log.Fatalf`** used at startup for unrecoverable errors (connection failures,
  system detection).
- **No sentinel errors** or error type assertions beyond checking `nil`.

### TypeScript

- **`throw new Error(...)`** for validation failures and missing requirements.
- **Promise-based** async with `async/await` throughout.
- **No custom error classes.**
- TypeScript SDK comments include `@throws` annotations on builder methods.

### Pattern: Errors Do Not Cross SDK Boundaries

Each SDK handles errors locally. The config binary (Rust or Go) communicates with the CLI via
gRPC status codes, not by propagating language-specific error types. The gRPC layer acts as the
error boundary between components.

---

## Design Patterns

### Builder Pattern (Core Pattern)

The dominant design pattern across all three SDKs. Every artifact type uses a builder with:

1. **Constructor** that takes required fields.
2. **`with_*` methods** for optional fields (fluent chaining).
3. **`build()` method** that produces the final protobuf message or registers with context.

Example (Rust):
```rust
Artifact::new(name, steps, systems)
    .with_aliases(aliases)
    .with_sources(sources)
    .build(context)
    .await
```

This pattern is replicated identically in Go and TypeScript. The builders are:
`Artifact`, `ArtifactSource`, `ArtifactStep`, `Argument`, `Job`, `Process`,
`DevelopmentEnvironment`, `UserEnvironment`, `OciImage`.

### Configuration Layering Pattern

Settings resolution follows a three-layer precedence model:

```
CLI flags > Project config (Vorpal.toml) > User config (~/.vorpal/settings.json) > Built-in defaults
```

Implemented via `ResolvedSettings::resolve()` in Rust with `SettingsSource` provenance tracking.

### gRPC Service Pattern

- Services defined in protobuf with `tonic` (Rust) and `grpc-js` (Go/TypeScript).
- Server-side streaming for long-running operations (`PrepareArtifact`, `BuildArtifact`).
- Interceptors for authentication (`apply_auth_to_request`).
- Channel-based message passing (`mpsc::channel`) for streaming responses.

### Content-Addressable Storage

Artifacts are identified by SHA-256 digest of their JSON-serialized protobuf message. This is
the core identity mechanism and must produce identical digests across all three SDKs.

### Deterministic Output

Multiple patterns ensure deterministic artifact digests:
- Secrets sorted by name before building (`self.secrets.sort_by(...)`).
- Symlinks sorted by source path.
- `BTreeMap` used for ordered iteration (Rust credentials).
- `SortedKeys()` helper in Go for deterministic map iteration.

---

## Module Organization

### Rust Workspace

```
Cargo.toml              # Workspace root (members: cli, config, sdk/rust)
cli/                    # vorpal-cli binary
  src/
    main.rs             # Entry point (tokio::main, delegates to command::run())
    command.rs          # CLI command definitions (clap), dispatch logic
    command/
      build.rs          # Build command implementation
      config.rs         # Config layer resolution, VorpalConfig structs
      config_cmd.rs     # Config get/set/show subcommands
      init.rs           # Project initialization
      inspect.rs        # Artifact inspection
      lock.rs           # Lock file management
      run.rs            # Artifact execution
      start/            # Service startup
        auth.rs         # OAuth2 authentication
        registry/       # Registry backends (local, S3)
        worker.rs       # Worker service implementation
      store/            # Local store management
        archives.rs     # Compression (zstd)
        hashes.rs       # Hash computation
        notary.rs       # Secret encryption/decryption
        paths.rs        # Store path helpers
        temps.rs        # Temporary/sandbox file management
      system/           # System management
        keys.rs         # Key generation
        prune.rs        # Store cleanup
config/                 # vorpal-config binary
  src/
    main.rs             # Entry point, dispatches by artifact name
    artifact/           # Per-artifact build definitions (vorpal, vorpal-job, etc.)
sdk/rust/               # vorpal-sdk crate
  api/                  # Protobuf definitions (.proto files)
  build.rs              # Protobuf code generation (tonic-prost-build)
  src/
    lib.rs              # Public API (api, artifact, cli, context modules)
    artifact.rs         # Builder types (Artifact, Job, Process, etc.)
    artifact/           # Per-tool artifact definitions (one file per tool)
    cli.rs              # SDK CLI argument parsing
```

### Go SDK

```
sdk/go/
  go.mod
  cmd/vorpal/
    main.go             # Entry point, dispatches by artifact name
    artifact/           # Per-artifact build definitions
      builder.go        # Builder types (mirrors Rust artifact.rs)
      step.go           # Step helpers (shell, bash, bwrap)
      systems.go        # System constants
      vorpal.go         # Vorpal artifact definition
      vorpal_*.go       # Other artifact definitions
  pkg/
    api/                # Generated protobuf code (one package per service)
    artifact/           # Artifact builder utilities
    config/             # Context, command parsing, system detection
    store/              # Store path helpers (hash, path, sandbox)
```

### TypeScript SDK

```
sdk/typescript/
  package.json          # @altf4llc/vorpal-sdk
  tsconfig.json
  src/
    index.ts            # Public API re-exports
    artifact.ts         # Builder types (mirrors Rust artifact.rs)
    artifact/
      step.ts           # Step helpers
      language/         # Language-specific builders (go, rust, typescript)
    cli.ts              # CLI argument parsing
    context.ts          # ConfigContext implementation
    system.ts           # System detection utilities
    api/                # Generated protobuf types
```

### Pattern: Mirror Structure

The Go and TypeScript SDKs intentionally mirror the Rust SDK's module structure. Builder types,
step helpers, and language-specific artifact definitions are organized identically across all
three SDKs to make cross-SDK parity easier to maintain.

---

## Code Style Observations

### Rust-Specific

- **Edition 2021** across all crates.
- **Explicit version pinning** for all dependencies in `Cargo.toml` (no `^` or `~` ranges).
- **`serde_derive`** used via `features = ["serde_derive"]` rather than separate derive crate.
- **`skip_serializing_if = "Option::is_none"`** consistently applied to optional config fields.
- **Async throughout** — `tokio` runtime with multi-thread feature, `async fn` for all I/O.
- **Structured logging** via `tracing` crate (`info!`, `warn!`, `error!`) to stderr.
- **No `unwrap()` in library code** — `anyhow::Result` or `tonic::Status` used instead. A few
  `expect()` calls exist at initialization boundaries.

### Go-Specific

- **`flag` package** used for CLI argument parsing (not `cobra` or `urfave/cli`).
- **Function variable injection** for testability (`getKeyCredentialsPathFunc` pattern in auth
  tests).
- **Table-driven tests** with `t.Run()` subtests (standard Go testing idiom).
- **`defer` for cleanup** (file handle closing, mock restoration).
- **No interfaces defined** for dependency injection — concrete types used directly.
- **`log.Fatal` at boundaries**, `fmt.Errorf` with `%w` wrapping internally.

### TypeScript-Specific

- **JSDoc comments** with `@param`, `@returns`, `@throws` annotations on all public methods.
- **Private fields** use `_` prefix convention (not `#` private fields).
- **`this` return type** for fluent builder chaining.
- **`.js` extension** in import paths (required for ESM + Node16 module resolution).
- **`type` imports** used where only types are needed (`import type { ... }`).
- **No runtime type checking** beyond TypeScript's static analysis.

---

## Cross-SDK Parity Requirements

The three SDKs (Rust, Go, TypeScript) must produce **identical artifact digests** for the same
logical artifact definition. This imposes strict requirements:

1. **Script templates must be character-for-character identical.** The TypeScript SDK includes
   comments like `// Script template matches Rust formatdoc! in Process::build()` and
   `// CRITICAL: Shell script template must be character-for-character identical`.

2. **Sorting must be identical.** Secrets sorted by name, symlinks sorted by source path,
   artifacts sorted consistently.

3. **JSON serialization must match.** SHA-256 is computed over JSON-serialized protobuf messages.
   Field ordering, null handling, and number representation must align.

4. **Protobuf field numbering is frozen.** Changes to `.proto` files affect all three SDKs
   simultaneously.

This parity is currently verified by the `sdk-parity` skill which compares artifact digests
between Rust and Go SDK builds.

---

## Configuration File Conventions

| File            | Format | Purpose                                    |
|-----------------|--------|--------------------------------------------|
| `Vorpal.toml`   | TOML   | Project-level config (language, source, etc.) |
| `~/.vorpal/settings.json` | JSON | User-level settings (registry, namespace) |
| `Cargo.toml`    | TOML   | Rust workspace and crate manifests         |
| `go.mod`        | Go mod | Go module definition                       |
| `package.json`  | JSON   | TypeScript package manifest                |
| `tsconfig.json` | JSON   | TypeScript compiler configuration          |
| `rust-toolchain.toml` | TOML | Rust toolchain pinning                |

---

## Gaps & Observations

1. **No automated formatting enforcement.** rustfmt, gofmt, and prettier are not run in CI.
   No pre-commit hooks exist.

2. **No linter configuration files.** Clippy and go vet run with defaults. No custom rules
   or suppressions are codified (one `#[allow]` exception noted).

3. **No `.editorconfig`.** No shared editor settings for indentation, line endings, or
   trailing whitespace.

4. **No CI/CD pipeline visible.** No `.github/workflows/` directory exists in the repository.
   Quality gates, if any, are external to the codebase.

5. **`thiserror` dependency unused.** Listed in `cli/Cargo.toml` but no `#[derive(Error)]`
   types found. Consider removing or adopting for structured CLI errors.

6. **Inconsistent CLI parsing across SDKs.** Rust CLI uses `clap` (derive macros), Go uses
   raw `flag` package, TypeScript uses manual argument parsing. The Go and TypeScript parsers
   are for the config binary only (simpler use case).

7. **No `cargo-machete` in CI.** The SDK's `Cargo.toml` has `[package.metadata.cargo-machete]`
   with `ignored` entries, suggesting it has been used but is not automated.

8. **Comments are sparse.** Rust code has minimal inline comments. Go code uses standard
   godoc-style comments on exported types. TypeScript has the most comprehensive JSDoc
   annotations.
