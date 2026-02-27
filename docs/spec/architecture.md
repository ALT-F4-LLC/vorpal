# Architecture

> Project specification for Vorpal — generated from codebase analysis.

## Overview

Vorpal is a **content-addressed build system** that lets users define builds as real programs (not YAML/DSL) using SDKs in Rust, Go, or TypeScript. It handles hermetic execution, cross-platform targeting, content-addressed caching, and artifact distribution.

The system follows a client-server architecture with gRPC as the communication backbone, protobuf-defined APIs, and a local content-addressed store at `/var/lib/vorpal/`.

## Project Structure

```
vorpal/
├── cli/                    # CLI binary (vorpal-cli crate) — the main entry point
│   └── src/
│       ├── main.rs          # tokio::main, delegates to command::run()
│       ├── command.rs        # clap CLI definition, command dispatch
│       ├── command/
│       │   ├── build.rs      # `vorpal build` — orchestrates config→agent→worker flow
│       │   ├── config.rs     # Vorpal.toml parsing and layered config resolution
│       │   ├── config_cmd.rs # `vorpal config` subcommand (get/set/show)
│       │   ├── init.rs       # `vorpal init` — project scaffolding
│       │   ├── inspect.rs    # `vorpal inspect` — view artifact metadata
│       │   ├── lock.rs       # Vorpal.lock lockfile management
│       │   ├── run.rs        # `vorpal run` — execute built artifacts by alias
│       │   ├── start.rs      # `vorpal system services start` — gRPC server bootstrap
│       │   ├── start/
│       │   │   ├── agent.rs   # AgentService implementation
│       │   │   ├── auth.rs    # OIDC/OAuth2 JWT validation middleware
│       │   │   ├── registry/  # ArchiveService + ArtifactService (local & S3 backends)
│       │   │   └── worker.rs  # WorkerService implementation
│       │   ├── store/         # Local store utilities (paths, hashes, archives, notary)
│       │   ├── system.rs      # `vorpal system` subcommands
│       │   └── template/      # Project templates (go, rust, typescript)
│       └── ...
├── config/                 # vorpal-config crate — Vorpal's own build config
│   └── src/
│       ├── main.rs          # Config binary: dispatches to artifact builders
│       └── artifact/        # Build definitions for vorpal itself
│           ├── vorpal.rs
│           ├── vorpal_container_image.rs
│           ├── vorpal_job.rs
│           ├── vorpal_process.rs
│           ├── vorpal_release.rs
│           ├── vorpal_shell.rs
│           └── vorpal_user.rs
├── sdk/
│   ├── rust/               # vorpal-sdk crate — Rust SDK (canonical)
│   │   ├── api/             # Protobuf definitions (.proto files)
│   │   │   ├── agent/agent.proto
│   │   │   ├── archive/archive.proto
│   │   │   ├── artifact/artifact.proto
│   │   │   ├── context/context.proto
│   │   │   └── worker/worker.proto
│   │   ├── src/
│   │   │   ├── lib.rs        # SDK entry: api (tonic-generated), artifact, cli, context
│   │   │   ├── artifact.rs   # Artifact builder API (Artifact, Job, Process, etc.)
│   │   │   ├── artifact/     # Built-in artifact definitions (toolchains, languages)
│   │   │   ├── cli.rs        # SDK-side CLI parsing for config binaries
│   │   │   └── context.rs    # ConfigContext — client-side artifact graph + gRPC clients
│   │   └── build.rs          # tonic-prost-build for proto codegen
│   ├── go/                 # Go SDK — mirrors Rust SDK artifact builders
│   │   ├── cmd/vorpal/      # Go config binary entry point
│   │   ├── pkg/
│   │   │   ├── api/          # Generated Go protobuf/gRPC code
│   │   │   ├── artifact/     # Go artifact builder implementations
│   │   │   ├── config/       # Config context, system detection, paths
│   │   │   └── store/        # Hash, path, sandbox utilities
│   │   └── go.mod
│   └── typescript/         # TypeScript SDK — mirrors Rust SDK artifact builders
│       ├── src/
│       │   ├── api/          # Generated TypeScript protobuf/gRPC code
│       │   ├── artifact.ts   # Artifact builder API
│       │   ├── artifact/     # Language builders (go, rust, typescript)
│       │   ├── cli.ts        # SDK-side CLI parsing
│       │   ├── context.ts    # Config context (gRPC clients)
│       │   ├── system.ts     # System detection
│       │   └── vorpal.ts     # Main entry point
│       └── package.json
├── terraform/              # Keycloak IDP and worker provisioning
├── script/                 # Development and CI helper scripts
├── Cargo.toml              # Workspace: cli, config, sdk/rust
├── Vorpal.toml             # Vorpal's own build config (Rust)
├── Vorpal.go.toml          # Vorpal's own build config (Go)
├── Vorpal.ts.toml          # Vorpal's own build config (TypeScript)
└── Vorpal.lock             # Lockfile with pinned source digests per platform
```

## Cargo Workspace

The Rust codebase is organized as a Cargo workspace with three members:

| Crate | Binary | Purpose |
|---|---|---|
| `cli` (`vorpal-cli`) | `vorpal` | Main CLI — commands, services, store management |
| `config` (`vorpal-config`) | `vorpal-config` | Vorpal's own build configuration (dogfooding) |
| `sdk/rust` (`vorpal-sdk`) | — (library) | Publishable SDK for writing Vorpal configs |

`cli` depends on `sdk/rust` (via path). `config` depends on `sdk/rust` (via path). The SDK is the shared foundation; `cli` and `config` are leaf crates.

## Core Architecture

### Client-Server Model

Vorpal uses a **single binary** (`vorpal`) that serves multiple roles:

1. **CLI Client** — `vorpal build`, `vorpal run`, `vorpal inspect`, etc.
2. **gRPC Server** — `vorpal system services start` hosts one or more services.
3. **Config Executor** — builds and launches user config binaries as child processes.

Communication between client and server uses **gRPC over Unix domain sockets** (default: `/var/lib/vorpal/vorpal.sock`) or TCP with optional mTLS.

### Service Architecture

The gRPC server hosts up to four services, all routed through a single `tonic::Server`:

```
┌──────────────────────────────────────────────────┐
│                  gRPC Server                      │
│  (UDS: /var/lib/vorpal/vorpal.sock or TCP:23151) │
│                                                   │
│  ┌─────────────┐  ┌──────────────────────────┐   │
│  │ AgentService│  │     ArchiveService        │   │
│  │             │  │ (local FS or S3 backend)  │   │
│  └─────────────┘  └──────────────────────────┘   │
│  ┌──────────────────┐  ┌─────────────────────┐   │
│  │ ArtifactService  │  │   WorkerService      │   │
│  │ (metadata store) │  │ (build execution)    │   │
│  └──────────────────┘  └─────────────────────┘   │
│  ┌──────────────────┐                             │
│  │  HealthService   │  (optional, separate port)  │
│  └──────────────────┘                             │
└──────────────────────────────────────────────────┘
```

- **AgentService** — Prepares artifacts: resolves sources (HTTP download, local copy), computes content digests, pushes source archives to the registry, encrypts secrets, manages the lockfile. Streams `PrepareArtifactResponse` messages back to the client.
- **ArchiveService** — Content-addressed blob store. `Check` / `Pull` / `Push` operations for `.tar.zst` archives. Backends: local filesystem or AWS S3.
- **ArtifactService** — Artifact metadata store. `GetArtifact`, `GetArtifacts`, `StoreArtifact`, `GetArtifactAlias`. Backends: local filesystem or AWS S3.
- **WorkerService** — Executes build steps. Pulls sources and dependency artifacts from the registry, runs steps in sandboxed environments, compresses outputs, pushes results back to the registry. Streams `BuildArtifactResponse` log output.
- **HealthService** — Optional gRPC health checks (tonic-health). Can run on a separate plaintext TCP port when the main listener uses UDS or TLS.

Services are selectable at startup via `--services agent,registry,worker` (default: all three).

### Build Pipeline

The `vorpal build <name>` command orchestrates a multi-phase pipeline:

```
1. Parse Vorpal.toml config
   ↓
2. Build the config binary (using the appropriate SDK language builder)
   ↓  (Go/Rust/TypeScript → compiled binary via Agent→Worker)
3. Launch config binary as a child process with a ContextService gRPC server
   ↓
4. Config binary evaluates the user's build definition, registers artifacts
   via the ContextService gRPC API
   ↓
5. CLI queries ContextService for the artifact graph
   ↓
6. Topological sort of the dependency graph (petgraph)
   ↓
7. For each artifact in order:
   a. Check local store and registry cache
   b. If not cached: send BuildArtifactRequest to WorkerService
   c. Worker pulls sources, runs steps, pushes output archive
   d. CLI pulls output archive from registry to local store
   ↓
8. Print artifact digest (or path with --path flag)
```

### Config Binary Pattern

A key architectural decision: **build configurations are programs, not data files**. The flow:

1. User writes a config as a Go/Rust/TypeScript program using the Vorpal SDK.
2. `Vorpal.toml` declares the language and source files.
3. The CLI compiles this config into a binary using the appropriate language builder.
4. The CLI starts the compiled config binary as a subprocess.
5. The config binary uses SDK's `ConfigContext` to register artifacts via gRPC.
6. The config binary hosts a `ContextService` gRPC server exposing the registered artifacts.
7. The CLI connects back to the ContextService, reads the artifact graph, then kills the subprocess.

This pattern enables full programming language expressiveness in build definitions.

## API Contracts (Protobuf)

All inter-service communication uses Protocol Buffers v3. Proto source of truth lives in `sdk/rust/api/`.

### Services

| Service | Proto Package | Methods |
|---|---|---|
| `AgentService` | `vorpal.agent` | `PrepareArtifact` (server-streaming) |
| `ArchiveService` | `vorpal.archive` | `Check` (unary), `Pull` (server-streaming), `Push` (client-streaming) |
| `ArtifactService` | `vorpal.artifact` | `GetArtifact`, `GetArtifacts`, `StoreArtifact`, `GetArtifactAlias` |
| `ContextService` | `vorpal.context` | `GetArtifact`, `GetArtifacts` |
| `WorkerService` | `vorpal.worker` | `BuildArtifact` (server-streaming) |

### Key Data Models

**Artifact** — The core unit:
- `name`: Human-readable identifier
- `target`: `ArtifactSystem` enum (aarch64-darwin, aarch64-linux, x8664-darwin, x8664-linux)
- `sources`: List of `ArtifactSource` (name, path, digest, includes, excludes)
- `steps`: List of `ArtifactStep` (entrypoint, script, arguments, environments, secrets, artifact dependencies)
- `systems`: Supported target systems
- `aliases`: Named tags for lookup (namespace/name:tag format)

**ArtifactSource** — Source input for an artifact:
- Types: HTTP URL, local path, (git — declared but not yet implemented)
- `digest`: SHA-256 content hash for integrity verification
- `includes`/`excludes`: File filtering globs

**ArtifactStep** — A build step:
- `entrypoint`: Executable to run (e.g., `bash`, `bwrap`)
- `script`: Inline shell script
- `arguments`: CLI arguments
- `artifacts`: Dependency artifact digests (made available as `$VORPAL_ARTIFACT_<digest>`)
- `environments`: Environment variable overrides
- `secrets`: Encrypted key-value pairs (RSA public-key encrypted)

### Code Generation

Proto files are compiled to all three SDK languages:

| Language | Tool | Output |
|---|---|---|
| Rust | `tonic-prost-build` (build.rs) | `tonic::include_proto!()` in `sdk/rust/src/lib.rs` |
| Go | `protoc` + `protoc-gen-go` + `protoc-gen-go-grpc` | `sdk/go/pkg/api/` |
| TypeScript | `protoc` + `protoc-gen-ts_proto` | `sdk/typescript/src/api/` |

The `make generate` target regenerates Go and TypeScript code from the Rust-owned proto files.

## Content-Addressed Store

All artifacts are stored by their SHA-256 content digest. The local store layout:

```
/var/lib/vorpal/
├── vorpal.sock              # Unix domain socket
├── vorpal.lock              # Server instance lock (advisory flock)
├── key/
│   ├── ca.pem               # CA certificate
│   ├── ca.key.pem           # CA private key
│   ├── service.pem          # Server certificate
│   ├── service.key.pem      # Server private key
│   ├── service.public.pem   # RSA public key (for secret encryption)
│   ├── service.secret        # RSA secret material
│   └── credentials.json     # OAuth2 tokens
├── sandbox/                 # Temporary build workspaces (UUID-named)
│   └── <uuid>/
└── store/
    └── artifact/
        ├── alias/<namespace>/<system>/<name>/<tag>   # Alias → digest mapping
        ├── archive/<namespace>/<digest>.tar.zst       # Compressed archives
        ├── config/<namespace>/<digest>.json            # Artifact metadata
        └── output/<namespace>/<digest>/                # Unpacked artifact outputs
            ├── <digest>.lock.json                      # Build-in-progress lock
            └── <digest>/                               # Actual files
```

**Archive format**: tar + zstd compression (`.tar.zst`).

**Digest computation**: SHA-256 over the JSON-serialized `Artifact` protobuf message. Sources are hashed independently over their file contents.

**Namespace isolation**: All store paths include a namespace component (default: `library`), enabling multi-tenant artifact isolation.

## Execution Sandboxing

Build steps are sandboxed differently per platform:

| Platform | Method | Implementation |
|---|---|---|
| macOS (Darwin) | **Unsandboxed bash** | `step::bash()` — runs directly with `set -euo pipefail` |
| Linux | **bubblewrap (bwrap)** | `step::bwrap()` — `--unshare-all --share-net`, custom rootfs, read-only artifact mounts |

On Linux, `bwrap` provides namespace isolation:
- Unshares all namespaces (PID, mount, user, etc.) while sharing network
- Mounts a Vorpal-built Linux rootfs (`LinuxVorpal`) as the base filesystem
- Bind-mounts dependency artifacts read-only
- Runs as uid/gid 1000 inside the sandbox
- Build outputs written to `$VORPAL_OUTPUT`, workspace at `$VORPAL_WORKSPACE`

On macOS, builds run as plain bash scripts — no sandboxing beyond process isolation. This is a known gap.

## SDK Parity

All three SDKs (Rust, Go, TypeScript) implement equivalent artifact builders. CI enforces **digest parity** — every SDK must produce identical artifact digests for the same build configuration:

```
# From .github/workflows/vorpal.yaml
VORPAL_ARTIFACT=$(vorpal build "vorpal")
ARTIFACT=$(vorpal build --config "Vorpal.go.toml" "vorpal")
# Must match: $ARTIFACT == $VORPAL_ARTIFACT

ARTIFACT=$(vorpal build --config "Vorpal.ts.toml" "vorpal")
# Must match: $ARTIFACT == $VORPAL_ARTIFACT
```

Each SDK provides:
- Built-in toolchain artifacts (rustc, cargo, go, bun, nodejs, protoc, etc.)
- Language builders (`Go`, `Rust`, `TypeScript`) that compose toolchain artifacts into build pipelines
- `ConfigContext` for artifact registration and gRPC communication
- Builder pattern APIs for constructing artifacts

### Artifact Builder Hierarchy

The SDK provides a layered artifact abstraction:

1. **Toolchain artifacts** — Individual tools (e.g., `Rustc`, `Cargo`, `Go`, `Bun`, `Protoc`). Each has a hardcoded download URL and version.
2. **Compound artifacts** — Compositions (e.g., `RustToolchain` bundles rustc+cargo+clippy+rustfmt+rust-std+rust-src+rust-analyzer).
3. **Language builders** — High-level builders (e.g., `Rust::new().build()`) that compose toolchains, source compilation, and output packaging into a complete build pipeline.
4. **Semantic artifacts** — `Job`, `Process`, `DevelopmentEnvironment`, `UserEnvironment` — higher-level patterns built on top of the core `Artifact` primitive.

## Lockfile System

`Vorpal.lock` pins source digests per platform:

```toml
lockfile = 1

[[sources]]
name = "go"
path = "https://go.dev/dl/go1.26.0.darwin-arm64.tar.gz"
includes = []
excludes = []
digest = "83d3ebad7958766647de4d3a8a1b2d3860da607632ac07889085b933e3e97f0f"
platform = "aarch64-darwin"
```

- **Locked mode** (default): Sources must match their locked digest. Any change requires `--unlock`.
- **Unlock mode** (`--unlock`): Allows source updates. New digests are written back to the lockfile.
- Agent hydrates source digests from the lockfile before processing.
- HTTP sources are upserted into the lockfile immediately after preparation.
- Lock entries are keyed by `(name, platform)` tuple.

## Configuration System

Layered configuration resolution (highest to lowest priority):

1. **CLI flags** — Explicit `--registry`, `--worker`, `--namespace`, etc.
2. **Project config** — `Vorpal.toml` (or `--config` override)
3. **User config** — `~/.vorpal/settings.json`
4. **Built-in defaults** — Hardcoded in the CLI

Project config (`Vorpal.toml`) declares:
- `language` — `rust`, `go`, or `typescript`
- `name` — Config binary name
- `[source]` — Source includes and language-specific build options

Settings keys: `registry`, `worker`, `namespace`, `system`, `language`, `name`.

## Authentication & Authorization

- **Client authentication**: OAuth2 Device Authorization Grant flow (`vorpal login`). Tokens stored in `/var/lib/vorpal/key/credentials.json`. Automatic refresh token rotation.
- **Server authentication**: OIDC JWT validation via configurable issuer. JWKS auto-discovery.
- **Service-to-service**: OAuth2 Client Credentials flow for worker→registry communication.
- **Secret encryption**: RSA public-key encryption for artifact step secrets. Encrypted by the agent, decrypted by the worker.
- **IDP**: Keycloak (configured via Terraform and docker-compose for development).

## Cross-Platform Support

| Target | Build | Test | Sandbox |
|---|---|---|---|
| `aarch64-darwin` | macOS Apple Silicon | CI (macos-latest) | Bash (unsandboxed) |
| `x8664-darwin` | macOS Intel | CI (macos-latest-large) | Bash (unsandboxed) |
| `aarch64-linux` | Linux ARM64 | CI (ubuntu-latest-arm64) | Bubblewrap |
| `x8664-linux` | Linux x86_64 | CI (ubuntu-latest) | Bubblewrap |

Platform detection uses `uname` values, mapped to `ArtifactSystem` enum. The lockfile and source cache are keyed per platform.

## Key Architectural Decisions

1. **Config-as-code**: Build configs are compiled programs, not declarative files. This enables conditionals, loops, shared libraries, and IDE support in build definitions.

2. **Single binary**: One `vorpal` binary serves as CLI client, gRPC server, and orchestrator. Simplifies deployment and reduces coordination complexity.

3. **Content-addressed everything**: All artifacts identified by SHA-256 digest. Enables deterministic builds, cache sharing, and integrity verification.

4. **Proto-first API**: All service boundaries defined via protobuf. SDKs are generated from a single proto source. Enforces contract consistency across languages.

5. **Unix domain socket default**: Local communication avoids TCP overhead and TLS complexity for single-machine setups. TCP+TLS available for distributed deployments.

6. **SDK parity enforcement**: CI tests verify that all three SDKs produce identical digests. Prevents SDK drift.

7. **Zstd compression**: All archives use `tar.zst` for efficient compression and fast decompression of build artifacts.

8. **Epoch timestamps**: All file timestamps are normalized to Unix epoch (0) before hashing and archiving, ensuring reproducible digests regardless of build time.

## Known Gaps

- **macOS sandboxing**: Darwin builds run unsandboxed (plain bash). No namespace isolation equivalent to Linux bwrap.
- **Git source type**: Declared in the agent's source type enum but not yet implemented (`bail!("git not supported")`).
- **Parallel artifact preparation**: Agent processes sources sequentially. A TODO notes parallel support.
- **No incremental builds**: Artifact builds are all-or-nothing. No partial rebuild or fine-grained caching within a single artifact.
- **Single-worker execution**: No distributed worker pool or load balancing. One worker per server instance.
- **Lock file race conditions**: Multiple concurrent builds could race on lockfile writes (mitigated by advisory flock on the server socket, but config lockfile writes are not separately locked).
