---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "System architecture overview for the Vorpal build system"
owner: "@staff-engineer"
dependencies:
  - security.md
  - operations.md
---

# Architecture Specification

## 1. System Overview

Vorpal is a content-addressed, multi-language build system that enables hermetic, reproducible builds defined as code via SDKs in Rust, Go, and TypeScript. The system is organized as a set of gRPC services (agent, registry, worker) composed behind a single CLI binary, communicating over Unix domain sockets (local) or TCP with optional TLS (remote).

### Core Principles

- **Configuration as code**: Build definitions are written in real programming languages (Rust, Go, TypeScript) using Vorpal SDKs, not in declarative configuration files.
- **Content-addressed storage**: Artifacts are identified by SHA-256 digests of their inputs. Identical inputs produce identical digests, enabling aggressive caching.
- **Multi-platform support**: Four target systems are supported: `aarch64-darwin`, `aarch64-linux`, `x86_64-darwin`, `x86_64-linux`.
- **Sandboxed execution**: Linux builds run inside Bubblewrap (`bwrap`) sandboxes with namespace isolation. macOS builds execute via `bash` directly (no sandbox equivalent currently).

## 2. Repository Structure

```
vorpal/
├── cli/                    # CLI binary (vorpal-cli crate, binary name: vorpal)
│   └── src/
│       ├── main.rs         # Entry point, delegates to command::run()
│       ├── command.rs       # Clap CLI definition, all subcommands
│       └── command/
│           ├── build.rs     # `vorpal build` — orchestrates config → agent → worker → archive
│           ├── config.rs    # Vorpal.toml parsing, layered config resolution
│           ├── config_cmd.rs # `vorpal config` — get/set/show configuration
│           ├── init.rs      # `vorpal init` — project scaffolding
│           ├── inspect.rs   # `vorpal inspect` — artifact inspection
│           ├── lock.rs      # Lockfile management for source digests
│           ├── run.rs       # `vorpal run` — execute built artifacts
│           ├── start/       # `vorpal system services start` — gRPC server
│           │   ├── agent.rs     # AgentService implementation
│           │   ├── auth.rs      # OIDC JWT validation interceptor
│           │   ├── registry/    # ArchiveService + ArtifactService (local/S3 backends)
│           │   └── worker.rs    # WorkerService implementation
│           ├── store/       # Content-addressed local store operations
│           │   ├── archives.rs  # Compression/decompression (zstd, gzip, bzip2)
│           │   ├── hashes.rs    # SHA-256 digest computation for sources
│           │   ├── notary.rs    # Archive signing/verification
│           │   ├── paths.rs     # Store directory layout under /var/lib/vorpal/
│           │   └── temps.rs     # Sandbox/temp directory creation
│           ├── system/      # `vorpal system` — keys, prune
│           └── template/    # `vorpal init` templates (go, rust, typescript)
├── config/                 # Self-hosted build configuration (vorpal-config crate)
│   └── src/
│       ├── main.rs         # Dispatches to artifact builders based on artifact name
│       └── artifact/       # Build definitions for Vorpal's own artifacts
├── sdk/
│   ├── rust/               # vorpal-sdk crate (published to crates.io)
│   │   ├── api/            # Protobuf definitions (source of truth)
│   │   │   ├── agent/agent.proto
│   │   │   ├── archive/archive.proto
│   │   │   ├── artifact/artifact.proto
│   │   │   ├── context/context.proto
│   │   │   └── worker/worker.proto
│   │   ├── build.rs        # tonic-prost-build code generation
│   │   └── src/
│   │       ├── lib.rs       # Re-exports: api, artifact, cli, context
│   │       ├── artifact.rs  # Artifact builder API (Artifact, Job, Process, etc.)
│   │       ├── artifact/    # Pre-built artifact definitions (toolchains, tools)
│   │       ├── cli.rs       # SDK-side CLI (for config binaries)
│   │       └── context.rs   # ConfigContext — client-side build orchestration
│   ├── go/                 # Go SDK module
│   │   ├── cmd/vorpal/     # Go config binary entry point (mirrors config/)
│   │   └── pkg/
│   │       ├── api/         # Generated protobuf Go stubs
│   │       ├── artifact/    # Go artifact builder equivalents
│   │       ├── config/      # ConfigContext Go implementation
│   │       └── store/       # Store path/hash utilities
│   └── typescript/         # TypeScript SDK (npm package)
│       └── src/
│           ├── api/         # Generated protobuf TypeScript stubs
│           ├── artifact/    # TypeScript artifact builders
│           ├── cli.ts       # SDK-side CLI parsing
│           ├── context.ts   # ConfigContext TypeScript implementation
│           └── vorpal.ts    # Main entry point
├── script/                 # Shell scripts (install, dev, Lima, Linux builds)
├── terraform/              # Infrastructure as code (deployment)
├── Cargo.toml              # Workspace: cli, config, sdk/rust
├── Vorpal.toml             # Self-hosted build config (language=rust, source includes)
├── Vorpal.lock             # Lockfile for source digests
└── docker-compose.yaml     # Local Keycloak instance for auth development
```

## 3. Component Architecture

### 3.1 CLI (`vorpal-cli`)

The `vorpal` binary is the single user-facing entry point. It serves dual roles:

1. **Client commands**: `build`, `run`, `inspect`, `login`, `init`, `config` — these connect to running services as gRPC clients.
2. **Server command**: `system services start` — starts the gRPC server hosting agent, registry, and worker services.

All commands are defined via `clap` derive macros in `cli/src/command.rs`. The CLI supports layered configuration resolution:

- **Built-in defaults** (hardcoded)
- **User-level config** (`~/.vorpal/settings.json`)
- **Project-level config** (`Vorpal.toml`)
- **CLI flags** (highest precedence)

Settings fields: `registry`, `namespace`, `language`, `name`, `system`, `worker`.

### 3.2 gRPC Services

All inter-component communication uses gRPC (tonic). Protobuf definitions in `sdk/rust/api/` are the single source of truth; Go and TypeScript stubs are generated from them.

#### AgentService

- **RPC**: `PrepareArtifact` (server-streaming)
- **Role**: Orchestrates artifact preparation. Receives an artifact definition from the client, resolves sources (local filesystem, git, HTTP), computes content digests, checks the registry for cached results, and coordinates with the worker for builds that need execution.
- **Source handling**: Supports three source types — local directories, git repositories, and HTTP archives (tar.gz, zip, bzip2). Sources are hashed to produce content-addressed digests.
- **Lockfile integration**: The agent manages lockfiles (`Vorpal.lock`) for pinning source digests across builds.

#### ArtifactService (Registry)

- **RPCs**: `GetArtifact`, `GetArtifactAlias`, `GetArtifacts`, `StoreArtifact`
- **Role**: Metadata registry for artifact definitions. Stores and retrieves artifact protobuf messages keyed by digest. Supports alias resolution (`[namespace/]name[:tag]` format).
- **Backends**: Local filesystem or S3 (configurable via `--registry-backend`).

#### ArchiveService (Registry)

- **RPCs**: `Check`, `Pull` (server-streaming), `Push` (client-streaming)
- **Role**: Binary content store for artifact build outputs (archives). Content-addressed by digest. Streaming transfer for large archives.
- **Backends**: Local filesystem or S3 (same backend selection as ArtifactService).
- **Caching**: Archive check results are cached in-memory using `moka` with configurable TTL (default: 300 seconds).

#### WorkerService

- **RPC**: `BuildArtifact` (server-streaming)
- **Role**: Executes artifact build steps. Streams build output back to the caller. Handles sandbox creation, step execution (bash/bwrap/docker), output capture, archive compression (zstd), and result upload to the registry.
- **Sandbox isolation**: On Linux, builds run inside Bubblewrap (`bwrap`) with `--unshare-all --share-net`, bind-mounting artifact dependencies read-only. On macOS, builds execute via bash without sandboxing.
- **Service-to-service auth**: When an OIDC issuer is configured, the worker obtains service credentials via the OAuth2 Client Credentials Flow for authenticated communication with the registry.

#### ContextService

- **RPCs**: `GetArtifact`, `GetArtifacts`
- **Role**: Runs inside config binaries (not in the main server). When a user runs `vorpal build`, the CLI spawns the appropriate config binary (determined by `Vorpal.toml` language field). The config binary runs user-defined build logic using the SDK, then starts a ContextService gRPC server exposing the computed artifact graph back to the agent.

### 3.3 SDKs

All three SDKs provide equivalent functionality for defining build artifacts:

| Concept | Rust SDK | Go SDK | TypeScript SDK |
|---------|----------|--------|----------------|
| Build context | `ConfigContext` | `ConfigContext` | `ConfigContext` |
| Artifact definition | `Artifact`, `Job`, `Process`, `DevelopmentEnvironment`, `UserEnvironment` | `Builder`, language helpers | `Artifact`, language helpers |
| Step types | `bash()`, `bwrap()`, `shell()`, `docker()` | `Shell()`, `Docker()` | `shell()` |
| Pre-built toolchains | ~30 modules (rust_toolchain, go, bun, nodejs, protoc, etc.) | ~20 packages (mirroring Rust) | Language helpers |
| CLI parsing | `clap` derive (`Cli` struct in `cli.rs`) | `flag` package | CLI module |

**SDK workflow**: A config binary (written using the SDK) defines artifacts programmatically, including their sources, build steps, dependencies, and target systems. When invoked by the agent, the config binary resolves the artifact graph and starts a `ContextService` gRPC server so the agent can query the resulting artifacts.

### 3.4 Config Binary (`vorpal-config`)

The `config/` directory is Vorpal's self-hosted build configuration — it uses Vorpal to build itself. It demonstrates the pattern that all users follow:

1. A `Vorpal.toml` declares the project language and source includes.
2. A config binary (e.g., `config/src/main.rs`) uses the SDK to define artifacts.
3. The binary dispatches on the artifact name (e.g., `"vorpal"`, `"vorpal-shell"`, `"vorpal-release"`).
4. Each artifact module defines its build steps, dependencies, and target systems.

The same pattern exists in the `cli/src/command/template/` directory for Go, Rust, and TypeScript project templates (used by `vorpal init`).

## 4. Data Flow

### 4.1 Build Flow (`vorpal build <name>`)

```
User
  │
  ▼
vorpal CLI (client)
  │ 1. Parse Vorpal.toml, resolve config (user → project → defaults)
  │ 2. Determine language, spawn config binary
  │
  ▼
Config Binary (e.g., vorpal-config)
  │ 3. Execute user's build logic using SDK
  │ 4. Define artifact graph (artifacts, steps, sources, deps)
  │ 5. Start ContextService gRPC server on ephemeral port
  │
  ▼
AgentService (in vorpal server)
  │ 6. Receive PrepareArtifact request
  │ 7. Resolve sources (local/git/HTTP), compute content digests
  │ 8. Check registry for cached artifacts
  │ 9. If cache miss → delegate to WorkerService
  │
  ▼
WorkerService (in vorpal server)
  │ 10. Create sandbox (bwrap on Linux, bash on macOS)
  │ 11. Execute build steps sequentially
  │ 12. Compress output (zstd), upload archive to ArchiveService
  │ 13. Store artifact metadata in ArtifactService
  │ 14. Stream build output back to agent
  │
  ▼
vorpal CLI (client)
  15. Receive artifact digest
  16. Pull archive from registry
  17. Unpack to local store (/var/lib/vorpal/store/)
```

### 4.2 Run Flow (`vorpal run <alias>`)

```
vorpal CLI
  │ 1. Parse alias ([namespace/]name[:tag])
  │ 2. Resolve alias → digest via ArtifactService.GetArtifactAlias
  │ 3. Fetch artifact graph (recursive dependency resolution)
  │ 4. Pull archives from ArchiveService
  │ 5. Execute artifact binary from store path
```

## 5. Communication & Transport

### 5.1 Transport Modes

| Mode | Trigger | Use Case |
|------|---------|----------|
| Unix Domain Socket | Default (no `--port` flag) | Local development, single-machine builds |
| Plaintext TCP | `--port <N>` without `--tls` | Development/testing |
| TLS TCP | `--tls` (implies `--port`, default 23151) | Production, remote workers |

The default socket path is `/var/lib/vorpal/vorpal.sock`, overridable via `VORPAL_SOCKET_PATH` environment variable. Socket permissions are set to `0o660`. An advisory file lock prevents concurrent server instances.

### 5.2 Health Checks

A separate plaintext health check listener (default port 23152) can be enabled via `--health-check`. This runs independently of the main listener's TLS configuration, allowing load balancers to probe health without TLS client certs. Uses the standard gRPC health checking protocol (`grpc.health.v1`).

### 5.3 Service Composition

Services are selectively enabled via `--services` flag (default: `agent,registry,worker`). This allows running split deployments:

- **All-in-one** (default): Single server with all services.
- **Registry only**: `--services registry` — central artifact/archive store.
- **Worker only**: `--services worker` — build executor connecting to a remote registry.
- **Agent only**: `--services agent` — coordination layer.

## 6. Storage Layout

All persistent data lives under `/var/lib/vorpal/` (configurable via socket path override):

```
/var/lib/vorpal/
├── vorpal.sock              # Unix domain socket (runtime)
├── vorpal.lock              # Advisory lock file (runtime)
├── key/
│   ├── ca.pem               # CA certificate (TLS)
│   ├── ca.key.pem           # CA private key (TLS key generation)
│   ├── service.pem          # Server certificate (TLS)
│   ├── service.key.pem      # Server private key (TLS)
│   └── credentials.json     # OAuth2 token storage (login)
├── store/
│   └── <namespace>/
│       └── <digest>/
│           ├── archive.tar.zst    # Compressed build output
│           ├── config.json        # Artifact definition (protobuf as JSON)
│           ├── output/            # Unpacked artifact files
│           └── output.lock        # Exclusive lock during unpack
└── sandbox/
    └── <uuid>/              # Ephemeral build sandbox directories
        ├── output/          # $VORPAL_OUTPUT — build results go here
        └── workspace/       # $VORPAL_WORKSPACE — working directory
```

### Content Addressing

Artifacts are keyed by SHA-256 digest of their serialized protobuf definition (including all sources, steps, and dependency digests). This means:

- Identical build definitions produce identical digests.
- Any change to sources, steps, or dependencies produces a new digest.
- The store acts as an immutable cache — artifacts are never mutated, only added or pruned.

## 7. Authentication & Authorization

### OAuth2/OIDC Integration

- **Client authentication**: `vorpal login` implements the OAuth2 Device Authorization Grant (RFC 8628) flow. Tokens are stored in `/var/lib/vorpal/key/credentials.json` and attached as `Bearer` tokens to gRPC metadata.
- **Token refresh**: Automatic transparent refresh using refresh tokens when access tokens expire (5-minute buffer).
- **Server-side validation**: When `--issuer` is configured on the server, an OIDC JWT validation interceptor protects `ArtifactService`, `ArchiveService`, and `WorkerService` RPCs. JWKS keys are fetched from the issuer's discovery endpoint.
- **Service-to-service auth**: The worker can obtain service credentials via OAuth2 Client Credentials Flow when `--issuer-client-id` and `--issuer-client-secret` are configured.
- **Development mode**: Keycloak is provided via `docker-compose.yaml` for local OIDC development.

See `security.md` for detailed security analysis.

### Archive Signing

Archives are signed using RSA-SHA256 (`notary` module). The server's private key signs archive digests; clients can verify signatures using the server's public key. Keys are generated via `vorpal system keys generate`.

## 8. Build Execution Model

### Step Types

| Step Type | Entrypoint | Platform | Isolation |
|-----------|-----------|----------|-----------|
| `bash` | `bash` | macOS (Darwin) | None — executes in host environment |
| `bwrap` | `bwrap` | Linux | Bubblewrap namespace isolation (`--unshare-all --share-net`) |
| `docker` | `docker` | Any (requires Docker) | Docker container isolation |
| `shell` | Auto-selects | Cross-platform | `bash` on macOS, `bwrap` on Linux |

### Sandbox Environment Variables

Build steps receive these environment variables:
- `VORPAL_OUTPUT` — directory where build outputs must be placed
- `VORPAL_WORKSPACE` — working directory for the build
- `VORPAL_ARTIFACT_<digest>` — path to each dependency artifact in the store
- `HOME` — set to `$VORPAL_WORKSPACE`
- `PATH` — composed from dependency artifact `/bin` directories

### Linux Root Filesystem

Linux builds using `bwrap` require a root filesystem. The SDK provides `LinuxVorpal` — a multi-stage bootstrap that builds a minimal Debian-based rootfs from source. This rootfs is itself a content-addressed artifact, cached and reused across builds.

## 9. Pre-built Artifact Catalog

The Rust SDK ships ~30 pre-built artifact modules in `sdk/rust/src/artifact/`:

**Toolchains**: `rust_toolchain`, `rustc`, `rust_std`, `rust_src`, `rustfmt`, `rust_analyzer`, `go`, `nodejs`, `bun`

**Build tools**: `cargo`, `crane`, `clippy`, `protoc`, `protoc_gen_go`, `protoc_gen_go_grpc`

**Developer tools**: `gh` (GitHub CLI), `git`, `grpcurl`, `goimports`, `gopls`, `staticcheck`, `rsync`, `pnpm`

**System**: `linux_debian`, `linux_vorpal`, `linux_vorpal_slim`, `oci_image`

**Language builders**: `language/go.rs`, `language/rust.rs`, `language/typescript.rs` — high-level builders that compose toolchains and build steps for each language.

The Go SDK mirrors most of these in `sdk/go/pkg/artifact/`. The TypeScript SDK provides language-level helpers.

## 10. Configuration System

### Vorpal.toml

Project configuration file parsed by the CLI:

```toml
language = "rust"       # Config binary language (rust|go|typescript)
name = "vorpal-config"  # Config binary package/module name

[source]
includes = ["config", "sdk/rust"]   # Directories to include as sources

[source.rust]
packages = ["vorpal-config", "vorpal-sdk"]  # Rust packages to build
```

### Layered Resolution

Settings are resolved in priority order:
1. CLI flags (explicit user intent, highest priority)
2. `Vorpal.toml` project config
3. `~/.vorpal/settings.json` user config
4. Built-in defaults (lowest priority)

The `vorpal config` subcommand manages both user-level and project-level settings with `get`, `set`, and `show` operations.

## 11. Dependency Graph

### Internal Dependencies

```
vorpal-cli
  └── vorpal-sdk (path dependency)

vorpal-config
  └── vorpal-sdk (path dependency)
```

The workspace contains three crates: `cli`, `config`, `sdk/rust`. The CLI and config binary both depend on the SDK. The SDK is published to crates.io as `vorpal-sdk` (version `0.1.0-alpha.0`, Apache-2.0 license).

### Key External Dependencies

| Dependency | Purpose | Crate |
|-----------|---------|-------|
| tonic + prost | gRPC framework + protobuf | `vorpal-sdk`, `vorpal-cli` |
| tokio | Async runtime | All crates |
| clap | CLI argument parsing | `vorpal-cli`, `vorpal-sdk` |
| aws-sdk-s3 | S3 registry backend | `vorpal-cli` |
| oauth2 + jsonwebtoken | OIDC auth flows | `vorpal-cli`, `vorpal-sdk` |
| moka | In-memory cache (archive checks) | `vorpal-cli` |
| petgraph | Dependency graph topological sort | `vorpal-cli` |
| sha256 | Content-address digest computation | `vorpal-cli`, `vorpal-sdk` |
| async-compression + tokio-tar | Archive handling | `vorpal-cli` |
| rcgen | TLS certificate generation | `vorpal-cli` |
| bubblewrap (external) | Linux sandbox (not a Rust dep) | Runtime |

## 12. Cross-Platform Build Model

The system supports cross-platform targeting through the `ArtifactSystem` enum:

```protobuf
enum ArtifactSystem {
    UNKNOWN_SYSTEM = 0;
    AARCH64_DARWIN = 1;
    AARCH64_LINUX = 2;
    X8664_DARWIN = 3;
    X8664_LINUX = 4;
}
```

Artifacts declare which systems they support. The CLI's `--system` flag (default: host system) selects the target. The build step type is automatically selected based on the target platform (`shell()` function in `step.rs`).

Cross-compilation (building Linux artifacts on macOS or vice versa) requires either:
- A remote worker running on the target platform
- Lima VM for Linux builds on macOS (supported via `make lima*` targets)

## 13. Known Gaps and Limitations

1. **No macOS sandboxing**: Darwin builds execute without isolation. There is no equivalent to Bubblewrap on macOS without significant complexity (sandbox-exec is deprecated, virtualization is heavyweight).

2. **Sequential step execution**: Build steps within an artifact execute sequentially. There is a `TODO` comment noting parallel execution is not yet implemented (`context.rs:337`).

3. **No artifact garbage collection policy**: `vorpal system prune` exists but is manual. There is no automatic eviction policy based on age or disk usage.

4. **Single-server architecture**: While services can be split, there is no built-in service discovery, load balancing, or horizontal scaling for workers.

5. **Lockfile TODO**: The lockfile system has incomplete features. `context.rs:425` has a TODO for looking up artifacts in the lockfile during fetch.

6. **Credentials file**: `command.rs:628` has a TODO for loading existing credentials when writing new ones (currently overwrites the file).

7. **Docker step**: The docker step type does not support secrets (`step.rs:257` TODO).

8. **TypeScript SDK**: Has the thinnest feature set of the three SDKs — fewer pre-built artifact modules and less mature toolchain support compared to Rust and Go SDKs.
