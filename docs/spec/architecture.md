# Architecture Specification

This document describes the architecture of Vorpal as it exists in the codebase. It is derived
from the actual source code, not aspirational goals.

## Overview

Vorpal is a language-agnostic, distributed build system that enables declarative artifact
definitions across Rust, Go, and TypeScript. Users describe build artifacts using SDK builders
in their preferred language, and Vorpal orchestrates compilation, caching, and distribution
through a set of gRPC-based services.

The system follows a client-server architecture where the CLI acts as an orchestrator, and
three backend services (Agent, Registry, Worker) handle artifact preparation, storage, and
execution respectively.

## System Components

### 1. CLI (`cli/`)

The CLI is the primary user-facing binary (`vorpal`), built as a Rust binary crate
(`vorpal-cli`). It serves as the build orchestrator and provides all user commands.

**Entry point:** `cli/src/main.rs`

**Subcommands:**

| Command | Module | Purpose |
|---------|--------|---------|
| `build` | `command/build.rs` | Build an artifact from a Vorpal config |
| `config` | `command/config_cmd.rs` | Manage settings (get/set/show) |
| `init` | `command/init.rs` | Scaffold a new Vorpal project |
| `inspect` | `command/inspect.rs` | Inspect an artifact by digest |
| `login` | `command.rs` (inline) | OAuth2 device-code authentication |
| `run` | `command/run.rs` | Execute a built artifact from the store |
| `system keys` | `command/system/keys.rs` | Generate TLS key material |
| `system prune` | `command/system/prune.rs` | Clean up local store resources |
| `system services start` | `command/start.rs` | Start backend services |

**Key architectural decisions:**
- Uses `clap` for argument parsing with derive macros.
- Tracing (`tracing`/`tracing-subscriber`) for structured logging to stderr.
- Layered configuration: built-in defaults < user config (`~/.vorpal/settings.json`) <
  project config (`Vorpal.toml`) < CLI flags.
- The build command compiles user-defined configs into standalone binaries, spawns them as
  child processes, queries their artifacts via gRPC, then orchestrates the full build pipeline.

### 2. Config Crate (`config/`)

A standalone Rust binary crate (`vorpal-config`) that serves as the Vorpal project's own
build configuration. It defines how Vorpal builds itself (the "vorpal" artifact, container
images, releases, shells, jobs, etc.).

**Entry point:** `config/src/main.rs`

**Artifacts defined:**
- `vorpal` -- the main binary
- `vorpal-container-image` -- Docker container image
- `vorpal-job` -- CI job artifact
- `vorpal-process` -- process management artifact
- `vorpal-release` -- release artifact
- `vorpal-shell` -- development shell environment
- `vorpal-user` -- user environment

This crate uses the Rust SDK (`vorpal-sdk`) and demonstrates the "config-as-code" pattern
where the build configuration is itself a compiled program.

### 3. SDK (`sdk/`)

The SDK provides libraries for defining Vorpal artifacts in multiple languages. All SDKs
share the same protobuf contract and produce identical artifact digests for the same
configuration.

#### 3.1 Rust SDK (`sdk/rust/`)

The primary SDK, published as the `vorpal-sdk` crate.

**Modules:**
- `api` -- Generated protobuf/gRPC types via `tonic`/`prost` (built from `.proto` files at
  `sdk/rust/api/`).
- `artifact` -- Builder types (`Artifact`, `ArtifactStep`, `ArtifactSource`, `Job`,
  `Process`, `ProjectEnvironment`, `UserEnvironment`) and tool artifacts (Bun, Cargo,
  Go, Rust toolchain, Node.js, pnpm, protoc, etc.).
- `artifact/language/` -- Language-specific build pipelines: `go.rs`, `rust.rs`,
  `typescript.rs`.
- `artifact/step.rs` -- Step execution strategies: `bash` (macOS), `bwrap` (Linux sandbox
  via Bubblewrap), `shell` (auto-selects based on OS), `docker`.
- `artifact/system.rs` -- Platform detection and `ArtifactSystem` enum mapping.
- `cli.rs` -- Shared CLI argument definitions for config binaries (the `Start` command that
  config binaries implement).
- `context.rs` -- `ConfigContext` for managing artifact state, gRPC channel construction,
  TLS configuration, credential management, and the `ContextService` gRPC server.

**Key design patterns:**
- Builder pattern used extensively (e.g., `Artifact::new(...).with_sources(...).build(ctx)`).
- Artifact digests are SHA-256 hashes of JSON-serialized artifact definitions, ensuring
  content-addressable identity.
- `ConfigContext` acts as both a client (to Agent/Registry services) and a server (exposing
  `ContextService` for the CLI to query artifacts).

#### 3.2 Go SDK (`sdk/go/`)

A Go module at `github.com/ALT-F4-LLC/vorpal/sdk/go` that mirrors the Rust SDK's builder
pattern.

**Structure:**
- `pkg/api/` -- Generated protobuf/gRPC code (via `protoc-gen-go` and `protoc-gen-go-grpc`).
- `pkg/artifact/` -- Builder types mirroring the Rust SDK.
- `pkg/artifact/language/` -- Language-specific builders.
- `pkg/config/` -- Context management, system detection, credential handling.
- `pkg/store/` -- Local store path management.
- `cmd/vorpal/` -- The Go config binary entry point (parallel to `config/src/main.rs`).

**Parity enforcement:** CI validates that Go SDK builds produce identical artifact digests
to Rust SDK builds for the same configurations.

#### 3.3 TypeScript SDK (`sdk/typescript/`)

Published as `@vorpal/sdk` on npm. Uses Bun as the runtime and `ts-proto` for protobuf
code generation.

**Structure:**
- `src/api/` -- Generated gRPC client code from protobuf via `ts-proto`.
- `src/artifact.ts` -- Builder types (`ArtifactBuilder`, `JobBuilder`,
  `ProjectEnvironmentBuilder`, `UserEnvironmentBuilder`, etc.).
- `src/artifact/step.ts` -- Step execution strategies (`bash`, `bwrap`, `shell`, `docker`).
- `src/artifact/language/` -- Language builders (`rust.ts`, `typescript.ts`).
- `src/context.ts` -- `ConfigContext` with gRPC client/server, artifact store management,
  alias parsing.
- `src/cli.ts` -- CLI argument parsing for config binaries.
- `src/system.ts` -- Platform detection utilities.
- `src/index.ts` -- Public API re-exports.

**Dependencies:** `@grpc/grpc-js`, `@bufbuild/protobuf`, `smol-toml`.

**Build pipeline:** TypeScript configs are compiled to standalone executables via
`bun build --compile`, producing a single binary placed in the artifact output.

## gRPC API Contracts

All inter-service communication uses gRPC with protobuf. The `.proto` source of truth is at
`sdk/rust/api/`.

### Services

| Service | Proto Package | Description |
|---------|---------------|-------------|
| `AgentService` | `vorpal.agent` | Prepares artifacts (resolves sources, manages lockfiles) |
| `ArchiveService` | `vorpal.archive` | Stores and retrieves artifact archives (tar.zst) |
| `ArtifactService` | `vorpal.artifact` | Stores and retrieves artifact metadata, alias resolution |
| `ContextService` | `vorpal.context` | Exposes artifact definitions from config binaries |
| `WorkerService` | `vorpal.worker` | Executes artifact build steps in sandboxed environments |

### Key Messages

**`Artifact`** -- The core data model:
```
Artifact {
  target: ArtifactSystem       // Target platform for this build
  sources: [ArtifactSource]    // Source inputs (local paths, URLs, git repos)
  steps: [ArtifactStep]        // Build steps to execute
  systems: [ArtifactSystem]    // Platforms this artifact supports
  aliases: [string]            // Name:tag aliases for resolution
  name: string                 // Human-readable artifact name
}
```

**`ArtifactSystem`** enum: `AARCH64_DARWIN`, `AARCH64_LINUX`, `X8664_DARWIN`, `X8664_LINUX`.

**`ArtifactStep`** -- A single build step:
```
ArtifactStep {
  entrypoint: string           // Executor binary (bash, bwrap, docker)
  script: string               // Script to execute
  secrets: [ArtifactStepSecret]// Build-time secrets
  arguments: [string]          // Arguments to entrypoint
  artifacts: [string]          // Dependency artifact digests
  environments: [string]       // Environment variables (KEY=value)
}
```

## Build Pipeline

The build pipeline has two phases: **config resolution** and **artifact building**.

### Phase 1: Config Resolution

1. CLI reads `Vorpal.toml` to determine language, name, and source configuration.
2. CLI selects the appropriate language builder (Rust, Go, or TypeScript).
3. The language builder compiles the user's config source into a standalone binary.
4. For Rust: `cargo build` via the Rust language builder.
5. For Go: `go build` via the Go language builder.
6. For TypeScript: `bun build --compile` via the TypeScript language builder.
7. CLI spawns the compiled config binary as a child process with `start` arguments.
8. Config binary registers artifacts with the Agent service, then exposes them via
   `ContextService` on a random TCP port.
9. CLI connects to the config's `ContextService`, enumerates all registered artifacts,
   and kills the config process.

### Phase 2: Artifact Building

1. CLI constructs a dependency graph using `petgraph` and performs topological sort.
2. For each artifact in dependency order:
   a. Check if artifact output already exists locally (by digest).
   b. Attempt to pull from the Registry (archive service).
   c. If not cached, send build request to the Worker service.
   d. Worker executes the artifact's steps in a sandbox.
   e. Worker pushes the resulting archive to the Registry.
   f. CLI pulls the archive back from the Registry and unpacks it locally.

### Artifact Identity

Artifacts are content-addressed by their SHA-256 digest. The digest is computed over the
JSON serialization of the `Artifact` protobuf message. This means:
- Identical configurations always produce the same digest.
- Any change to sources, steps, or dependencies produces a new digest.
- The digest serves as both the cache key and the storage key.

## Service Architecture

### Service Lifecycle

All three services (Agent, Registry, Worker) run in a single process, started via
`vorpal system services start`. They share a single gRPC server (either TCP or Unix domain
socket).

**Transport options:**
- **Unix domain socket** (default): `/var/lib/vorpal/vorpal.sock` (overridable via
  `VORPAL_SOCKET_PATH` env var). Used for local development.
- **TCP**: Enabled via `--port` flag or when `--tls` is set (defaults to port 23151).
- **TLS**: Optional, requires pre-generated key material in `/var/lib/vorpal/key/`.

**Concurrency control:**
- Advisory file lock (`/var/lib/vorpal/vorpal.lock`) prevents multiple instances.
- Stale socket detection with connection probe before cleanup.
- Graceful shutdown on SIGINT/SIGTERM.

### Agent Service

Runs on the same host as the CLI. Responsibilities:
- Source resolution: local paths, HTTP URLs, git repositories.
- Source digesting and lockfile management (`Vorpal.lock`).
- Archive compression (zstd) and upload to the Registry.
- Artifact source caching.

Source types: `Local` (filesystem path), `Http` (URL download with archive extraction),
`Git` (repository clone).

### Registry Service

Persists artifact metadata and archives. Two storage backends:

| Backend | Flag | Description |
|---------|------|-------------|
| `local` | `--registry-backend local` | Filesystem storage under `/var/lib/vorpal/store/` |
| `s3` | `--registry-backend s3` | AWS S3 storage (requires `--registry-backend-s3-bucket`) |

**Sub-services:**
- `ArchiveService`: Streams artifact archives (tar.zst) for push/pull operations. Supports
  archive check caching via `moka` with configurable TTL.
- `ArtifactService`: Stores/retrieves artifact definitions (JSON) and manages aliases
  (namespace/name:tag -> digest mapping).

### Worker Service

Executes build steps. Responsibilities:
- Pulls dependency artifacts from the Registry.
- Sets up sandbox environment with artifact dependencies mounted.
- Executes steps using the configured entrypoint (bash, bwrap, docker).
- Compresses build output and pushes to the Registry.
- Stores the artifact definition in the Registry.

**Linux sandboxing:** On Linux, the `bwrap` (Bubblewrap) step type provides namespace
isolation with `--unshare-all --share-net`, read-only bind mounts for dependencies,
and a custom rootfs from the `linux-vorpal` artifact.

**macOS execution:** On macOS, steps run directly via `bash` without sandboxing.

### Health Checks

Optional plaintext gRPC health check endpoint (separate port, default 23152) for load
balancer integration. Uses `tonic-health`.

## Local Store Layout

All persistent state lives under `/var/lib/vorpal/`:

```
/var/lib/vorpal/
  key/
    ca.pem               # CA certificate
    ca.key.pem           # CA private key
    service.pem          # Service certificate
    service.key.pem      # Service private key
    service.public.pem   # Service public key (for notary/signing)
    service.secret       # Service secret
    credentials.json     # OAuth2 credentials
  sandbox/
    <uuid>/              # Temporary build sandboxes (UUID v7)
  store/
    artifact/
      alias/
        <namespace>/
          <system>/
            <name>/
              <tag>      # File containing digest string
      archive/
        <namespace>/
          <digest>.tar.zst
      config/
        <namespace>/
          <digest>.json
      output/
        <namespace>/
          <digest>/      # Unpacked artifact output
          <digest>.lock.json
  vorpal.sock            # Unix domain socket
  vorpal.lock            # Advisory file lock
```

## Configuration System

### Vorpal.toml (Project Config)

```toml
language = "rust"          # Language of the config source: rust | go | typescript
name = "vorpal-config"     # Config artifact name

[source]
includes = ["config", "sdk/rust"]  # Paths to include in the source

[source.rust]
packages = ["vorpal-config", "vorpal-sdk"]  # Rust packages to build

[source.go]
directory = "sdk/go/cmd/vorpal"  # Go module directory

[source.typescript]
entrypoint = "sdk/typescript/src/vorpal.ts"  # TypeScript entry point
bun_version = "1.2.1"  # Optional Bun version override
```

### Layered Settings

Settings are resolved with precedence: CLI flags > project config > user config > defaults.

Configurable settings: `registry`, `namespace`, `language`, `name`, `system`, `worker`.

User config is stored as JSON at `~/.vorpal/settings.json`.

### Lockfile (Vorpal.lock)

TOML-formatted file tracking source digests per platform for reproducible builds:

```toml
lockfile = 1

[[sources]]
name = "vorpal-config"
path = "."
digest = "abc123..."
platform = "aarch64-darwin"
```

## Authentication and Authorization

### OAuth2 / OIDC

- **Device code flow** for CLI login (`vorpal login`).
- **Client credentials flow** for service-to-service auth (Worker -> Registry).
- Token storage in `/var/lib/vorpal/key/credentials.json`.
- Automatic token refresh with 5-minute expiry buffer.
- OIDC discovery via `.well-known/openid-configuration`.
- JWT validation with JWKS key rotation support.

### Authorization Model

JWT claims include a `namespaces` field mapping namespace names to permission arrays.
The server validates namespace-level permissions for artifact and archive operations.

### Service Authentication

The Registry and Worker services support optional OIDC-based authentication via gRPC
interceptors. When `--issuer` is provided at startup, all incoming requests require a
valid Bearer token. Without `--issuer`, services run without authentication.

## Dependency Graph

```
vorpal-cli (binary)
  -> vorpal-sdk (library)

vorpal-config (binary, self-build config)
  -> vorpal-sdk (library)

vorpal-sdk (library)
  -> tonic/prost (gRPC)
  -> protobuf definitions (sdk/rust/api/*.proto)
```

The CLI depends on the SDK for API types, builder abstractions, and context management.
The config crate also depends on the SDK to define Vorpal's own build artifacts.

## Cross-Platform Support

| Platform | Build | Sandbox | Status |
|----------|-------|---------|--------|
| aarch64-darwin (Apple Silicon macOS) | bash | None | Supported |
| x86_64-darwin (Intel macOS) | bash | None | Supported |
| aarch64-linux (ARM64 Linux) | bwrap | Bubblewrap | Supported |
| x86_64-linux (x86_64 Linux) | bwrap | Bubblewrap | Supported |

Linux builds use Bubblewrap for namespace isolation. macOS builds run directly via bash
without sandboxing. The `linux-vorpal` artifact provides a custom rootfs for Linux sandbox
environments, built in stages from a Debian base.

## Tool Artifact Catalog

The Rust SDK includes pre-built artifact definitions for common development tools:

| Artifact | Module | Description |
|----------|--------|-------------|
| `bun` | `artifact/bun.rs` | Bun runtime (default: v1.2.0) |
| `cargo` | `artifact/cargo.rs` | Cargo package manager |
| `clippy` | `artifact/clippy.rs` | Rust linter |
| `crane` | `artifact/crane.rs` | Container image tool |
| `gh` | `artifact/gh.rs` | GitHub CLI |
| `git` | `artifact/git.rs` | Git version control |
| `go` | `artifact/go.rs` | Go toolchain |
| `goimports` | `artifact/goimports.rs` | Go imports formatter |
| `gopls` | `artifact/gopls.rs` | Go language server |
| `grpcurl` | `artifact/grpcurl.rs` | gRPC command-line tool |
| `linux-debian` | `artifact/linux_debian.rs` | Debian base rootfs |
| `linux-vorpal` | `artifact/linux_vorpal.rs` | Vorpal Linux rootfs (multi-stage) |
| `linux-vorpal-slim` | `artifact/linux_vorpal_slim.rs` | Minimal Vorpal Linux rootfs |
| `nodejs` | `artifact/nodejs.rs` | Node.js runtime |
| `oci-image` | `artifact/oci_image.rs` | OCI container image builder |
| `pnpm` | `artifact/pnpm.rs` | pnpm package manager |
| `protoc` | `artifact/protoc.rs` | Protocol Buffers compiler |
| `protoc-gen-go` | `artifact/protoc_gen_go.rs` | Go protobuf plugin |
| `protoc-gen-go-grpc` | `artifact/protoc_gen_go_grpc.rs` | Go gRPC plugin |
| `rsync` | `artifact/rsync.rs` | File synchronization |
| `rust-analyzer` | `artifact/rust_analyzer.rs` | Rust language server |
| `rust-src` | `artifact/rust_src.rs` | Rust source code |
| `rust-std` | `artifact/rust_std.rs` | Rust standard library |
| `rust-toolchain` | `artifact/rust_toolchain.rs` | Rust toolchain (rustc + std + cargo) |
| `rustc` | `artifact/rustc.rs` | Rust compiler |
| `rustfmt` | `artifact/rustfmt.rs` | Rust formatter |
| `staticcheck` | `artifact/staticcheck.rs` | Go static analyzer |

## Infrastructure

### CI/CD (`.github/workflows/vorpal.yaml`)

Multi-stage pipeline across four runner configurations (macOS ARM64, macOS x86_64,
Ubuntu ARM64, Ubuntu x86_64):

1. **vendor** -- Cache Cargo dependencies, run `cargo check`.
2. **code-quality** -- Run `cargo fmt --check` and `cargo clippy`.
3. **build** -- Compile release binary, run tests, create distribution tarball.
4. **test** -- Integration test: install Vorpal, build all artifacts, validate Go SDK
   parity (same digests for same configs).
5. **container-image** -- Build and push Docker images (tag-triggered only).
6. **release** -- Create GitHub release with signed artifacts and attestations.

### Terraform (`terraform/`)

Infrastructure-as-code for deployment (minimal, references module).

### Lima (`lima.yaml`)

Virtual machine configuration for Linux development on macOS hosts.

### Docker Compose (`docker-compose.yaml`)

Keycloak instance for local OIDC/OAuth2 development and testing.

## Architectural Gaps and Known Issues

1. **No macOS sandboxing:** Build steps on macOS run unsandboxed via bash. There is no
   equivalent to the Linux Bubblewrap isolation.

2. **Single-process services:** All three services (Agent, Registry, Worker) run in a
   single process. Horizontal scaling requires running separate instances with the
   `--services` flag to select which services to enable.

3. **Limited TypeScript SDK parity:** The TypeScript SDK is newer and does not yet have
   CI parity validation (unlike Go SDK which validates digest matching against Rust SDK).

4. **Lockfile TODO items:** Several `TODO` comments in the codebase indicate lockfile
   handling is not fully complete (e.g., `context.rs:404` "look in lockfile for artifact
   version").

5. **No artifact garbage collection:** While `system prune` exists, there is no automatic
   garbage collection of unreferenced artifacts in the store.

6. **In-memory artifact store:** `ConfigContext` stores artifacts in a `HashMap` in memory.
   For very large dependency graphs, this could become a memory concern.

7. **Sequential artifact preparation:** A `TODO` in `context.rs:329` notes that artifact
   preparation should be parallelized.
