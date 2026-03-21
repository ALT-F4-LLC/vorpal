---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "System architecture overview of the Vorpal build system"
owner: "@staff-engineer"
dependencies:
  - security.md
  - operations.md
  - performance.md
---

# Architecture

Vorpal is a build system that treats build configurations as real programs. Users define builds in Rust, Go, or TypeScript using language-native SDKs. The system provides hermetic execution, content-addressed caching, cross-platform targeting, and artifact distribution through a client-server architecture built on gRPC.

## System Overview

Vorpal consists of three main layers:

1. **CLI** (`vorpal`) -- the user-facing command-line tool that orchestrates builds, manages system services, and runs artifacts.
2. **SDK** -- multi-language libraries (Rust, Go, TypeScript) that provide the API for defining build configurations as code.
3. **Services** -- backend gRPC services (Agent, Worker, Registry) that handle source preparation, artifact building, and artifact storage/distribution.

```
                          +-------------------+
                          |   User Config     |
                          | (Rust/Go/TS code) |
                          +--------+----------+
                                   |
                              SDK (language)
                                   |
                          +--------v----------+
                          |     vorpal CLI    |
                          +---+-----+-----+---+
                              |     |     |
                    +---------+  +--+--+  +---------+
                    |            |     |            |
               +----v----+ +----v----+ +----v------+
               |  Agent  | | Worker  | |  Registry |
               | Service | | Service | | (Archive  |
               |         | |         | |  Artifact)|
               +---------+ +---------+ +-----------+
```

## Component Architecture

### CLI (`cli/`)

The CLI is a Rust binary (`vorpal`) built with `clap` for argument parsing. It is the single entry point for all user interactions.

**Commands:**

| Command | Purpose |
|---------|---------|
| `build <name>` | Build an artifact by name from a `Vorpal.toml` config |
| `config` | Manage project and user-level configuration settings |
| `init <name>` | Scaffold a new Vorpal project with language selection |
| `inspect <digest>` | Inspect a stored artifact by its content digest |
| `login` | Authenticate via OAuth2 device flow (Keycloak) |
| `run <alias>` | Execute a previously built artifact by alias |
| `system keys generate` | Generate TLS key pairs for service communication |
| `system prune` | Clean up local artifact store (archives, outputs, configs, sandboxes, aliases) |
| `system services start` | Start the gRPC backend services |

**Configuration resolution** follows a three-layer precedence model (highest to lowest):
1. CLI flags (explicit)
2. Project-level config (`Vorpal.toml`)
3. User-level config (`~/.vorpal/settings.json`)
4. Built-in defaults

The `VorpalConfig` struct (`cli/src/command/config.rs`) defines all configurable settings: `registry`, `namespace`, `language`, `name`, `system`, `worker`. Each resolved value carries provenance tracking (`SettingsSource::Default | User | Project`).

### SDK (`sdk/`)

Multi-language SDK providing the programmatic interface for defining build artifacts.

#### Protobuf API (`sdk/rust/api/`)

All inter-service communication is defined via Protocol Buffers. Five service definitions exist:

| Proto Package | Service | Purpose |
|---------------|---------|---------|
| `vorpal.agent` | `AgentService` | Source preparation and artifact assembly |
| `vorpal.archive` | `ArchiveService` | Binary archive storage (push/pull/check) |
| `vorpal.artifact` | `ArtifactService` | Artifact metadata storage and retrieval |
| `vorpal.context` | `ContextService` | Artifact retrieval from config context |
| `vorpal.worker` | `WorkerService` | Artifact building and step execution |

Code generation:
- **Rust**: `tonic-prost-build` via `sdk/rust/build.rs`, with serde derives on artifact messages for JSON serialization.
- **Go**: `protoc` with `protoc-gen-go` / `protoc-gen-go-grpc` (generated into `sdk/go/pkg/api/`).
- **TypeScript**: `protoc-gen-ts_proto` (generated into `sdk/typescript/src/api/`).

#### Rust SDK (`sdk/rust/`)

Published as `vorpal-sdk` (Apache-2.0, `0.1.0-alpha.0`). Provides:

- **`artifact` module** -- Builder pattern types for defining artifacts:
  - `Artifact` -- core build unit with name, sources, steps, and target systems.
  - `ArtifactStep` -- individual build step with entrypoint, arguments, environments, and secrets.
  - `ArtifactSource` -- source input with content digest, includes/excludes, and path (local, HTTP, or git).
  - `DevelopmentEnvironment` -- generates shell activation scripts with environment variable management.
  - `UserEnvironment` -- generates user-wide tool installations with symlink management.
  - `Job` -- script-based build unit (wraps shell step).
  - `Process` -- long-running process artifact with start/stop/logs scripts.
- **`context` module** -- `ConfigContext` for managing the build session, artifact store, and gRPC client connections.
- **Language builders** (`artifact/language/`) -- `Rust`, `Go`, `TypeScript` builders that produce complete artifact definitions from high-level configuration.
- **Tool artifacts** -- Pre-built tool definitions for `bun`, `cargo`, `clippy`, `crane`, `gh`, `git`, `nodejs`, `protoc`, `rsync`, `rustc`, `rustfmt`, `staticcheck`, `gopls`, `goimports`, `grpcurl`, etc.

#### Go SDK (`sdk/go/`)

Mirrors the Rust SDK's artifact/config capabilities:
- `pkg/config/` -- `ConfigContext`, command parsing, system detection, path management.
- `pkg/artifact/` -- Builder types for `Go`, `Rust`, `TypeScript` languages, plus tools (`bun`, `protoc`, `crane`, `gh`, `git`, `nodejs`, etc.).
- `pkg/store/` -- Local store operations (sandbox, hash, path management).
- `cmd/vorpal/` -- Vorpal's own build configuration (self-hosting).

#### TypeScript SDK (`sdk/typescript/`)

Published as `@altf4llc/vorpal-sdk` on npm. Uses Bun as the runtime:
- `src/context.ts` -- Config context management.
- `src/artifact.ts` -- Artifact, step, and environment builder types.
- `src/artifact/language/` -- Language-specific builders (`typescript.ts`, `go.ts`, `rust.ts`).
- `src/system.ts` -- Platform detection and system enum mapping.

### Services

All services run within a single `vorpal` process, started via `vorpal system services start`. The service composition is configurable via `--services` flag (default: `agent,registry,worker`).

#### Transport

- **Unix Domain Socket** (default): `/var/lib/vorpal/vorpal.sock` (overridable via `VORPAL_SOCKET_PATH`).
- **TCP**: When `--port` is specified or `--tls` is enabled (default TLS port: `23151`).
- **TLS**: Optional, using self-signed certificates generated by `vorpal system keys generate`.
- **Health checks**: Optional plaintext TCP health endpoint (default port `23152`) using `tonic-health`.

A file-based advisory lock (`fs4`) prevents multiple instances from binding the same socket.

#### Agent Service (`cli/src/command/start/agent.rs`)

Responsible for preparing artifacts before building. Handles:

1. **Source resolution** -- Determines source type (local filesystem, HTTP URL, or git) and fetches content.
2. **Archive format detection** -- Auto-detects MIME types (gzip, bzip2, xz, zip, executables) and unpacks accordingly.
3. **Content hashing** -- Computes SHA-256 digests of source files for content-addressing.
4. **Lockfile management** -- Reads/writes `Vorpal.lock` to pin remote source digests per platform. Sources changed without `--unlock` flag are rejected.
5. **Secret encryption** -- Encrypts step secrets using the service's RSA public key before forwarding to workers.
6. **Source caching** -- In-memory cache (`SourceCache`) deduplicates HTTP source downloads within a session.
7. **Registry interaction** -- Pushes prepared source archives to the registry for worker consumption.

The agent streams `PrepareArtifactResponse` messages back to the CLI, providing progress updates and the final prepared artifact with its content digest.

#### Worker Service (`cli/src/command/start/worker.rs`)

Executes the actual build steps. Handles:

1. **Source pulling** -- Downloads source archives from the registry and unpacks them into a workspace.
2. **Dependency resolution** -- Pulls dependency artifact archives that build steps reference.
3. **Step execution** -- Runs each `ArtifactStep` as a subprocess with:
   - Environment variables: `VORPAL_OUTPUT` (output path), `VORPAL_WORKSPACE` (workspace path), `VORPAL_ARTIFACT_<digest>` (dependency paths), custom environments, decrypted secrets.
   - Script handling: Writes inline scripts to workspace, sets permissions, executes via entrypoint.
   - Variable expansion: Supports `$VAR` and `${VAR}` syntax in arguments and scripts.
4. **Output archiving** -- Compresses build output with zstd and pushes to the registry.
5. **Artifact storage** -- Registers the built artifact with its digest in the artifact service.
6. **Lock management** -- Creates/removes lock files to prevent concurrent builds of the same artifact.
7. **Service-to-service auth** -- Obtains OAuth2 client credentials for authenticated registry access.

Target system validation ensures workers only build artifacts matching the host platform.

#### Registry Services (`cli/src/command/start/registry/`)

Two sub-services compose the registry:

**ArchiveService** -- Binary blob storage for source and artifact archives:
- Backends: `local` (filesystem at `/var/lib/vorpal/`) or `s3` (AWS S3 bucket).
- Operations: `Check` (existence), `Pull` (streaming download), `Push` (streaming upload).
- Caching: Configurable TTL for archive check results (`--archive-cache-ttl`, default 300s).

**ArtifactService** -- Artifact metadata storage:
- Backends: `local` (filesystem) or `s3`.
- Operations: `GetArtifact`, `GetArtifacts`, `GetArtifactAlias`, `StoreArtifact`.
- Aliases: Named references (with system and tag) to artifact digests, enabling `vorpal run <alias>`.

#### Authentication (`cli/src/command/start/auth.rs`)

- **OIDC/OAuth2** -- Optional JWT validation via OIDC discovery. Uses Keycloak as the identity provider (see `docker-compose.yaml`, `terraform/module/keycloak/`).
- **Device flow** -- CLI login uses OAuth2 device authorization flow.
- **Client credentials** -- Worker-to-registry authentication uses OAuth2 client credentials flow.
- **Interceptors** -- gRPC interceptors validate JWT tokens on registry and worker endpoints when an issuer is configured.
- **Namespace authorization** -- JWT claims can restrict access to specific namespaces.

### Local Store (`cli/src/command/store/`)

On-disk storage layout under `/var/lib/vorpal/`:

| Module | Purpose |
|--------|---------|
| `archives` | zstd compression/decompression, zip unpacking |
| `hashes` | SHA-256 content digest computation for source files |
| `notary` | RSA encryption/decryption for build step secrets |
| `paths` | Path resolution for artifacts, archives, keys, sockets, aliases |
| `temps` | Sandbox directory/file creation for isolated build workspaces |

File timestamps are normalized (set to Unix epoch) to ensure reproducible content hashes regardless of when files were created.

## Data Model

### Core Types

```
Artifact
  |-- name: String
  |-- target: ArtifactSystem (host platform for this build)
  |-- systems: [ArtifactSystem] (platforms this artifact supports)
  |-- aliases: [String] (named references like "latest")
  |-- sources: [ArtifactSource]
  |     |-- name, path, digest, includes, excludes
  |-- steps: [ArtifactStep]
        |-- entrypoint, script, arguments
        |-- artifacts: [digest] (dependency references)
        |-- environments: [String] (KEY=VALUE)
        |-- secrets: [ArtifactStepSecret] (encrypted at rest)

ArtifactSystem: AARCH64_DARWIN | AARCH64_LINUX | X8664_DARWIN | X8664_LINUX
```

### Content Addressing

Artifacts are identified by SHA-256 digest of their serialized JSON representation. This digest serves as the cache key and storage path. Two artifacts with identical inputs always produce the same digest, enabling deterministic caching.

### Lockfile (`Vorpal.lock`)

JSON file tracking pinned source digests per platform. Sources are locked after first resolution and cannot change without `--unlock`. Format:

```json
{
  "lockfile": 1,
  "sources": [
    {
      "name": "source-name",
      "digest": "sha256-hex",
      "platform": "aarch64-darwin",
      "path": "https://...",
      "includes": [],
      "excludes": []
    }
  ]
}
```

## Build Flow

A `vorpal build <name>` invocation follows this sequence:

1. **Config resolution** -- CLI loads and merges `Vorpal.toml` (project) + `~/.vorpal/settings.json` (user) + defaults.
2. **Config compilation** -- Based on the configured language, the CLI:
   - Builds the config program itself (e.g., compiles the Rust/Go/TypeScript config source via the SDK's language builders).
   - Starts the compiled config binary as a subprocess exposing a `ContextService` gRPC server.
   - Queries the config for all artifact definitions.
3. **Dependency ordering** -- Artifacts are topologically sorted using `petgraph` based on step dependency references.
4. **For each artifact in order:**
   a. **Check cache** -- If the artifact output already exists locally, skip.
   b. **Pull from registry** -- Attempt to pull a cached archive from the registry.
   c. **Build via worker** -- If not cached, send the artifact to the worker service which:
      - Pulls sources and dependencies from the registry
      - Executes build steps in sandboxed workspaces
      - Archives and pushes the output back to the registry
5. **Output** -- Print the artifact digest (or path with `--path` flag).

## Platform Support

| Platform | Architecture | Status |
|----------|-------------|--------|
| macOS | Apple Silicon (aarch64) | Supported |
| macOS | Intel (x86_64) | Supported |
| Linux | x86_64 | Supported |
| Linux | ARM64 (aarch64) | Supported |

Platform detection uses `uname` values mapped to `ArtifactSystem` enum variants.

## Infrastructure

### Development Environment

- **Lima** -- Linux VM management for cross-platform testing (`lima.yaml`, `makefile` lima targets).
- **Docker Compose** -- Keycloak identity provider for local auth development.
- **Terraform** -- Keycloak realm/client configuration (`terraform/module/keycloak/`), worker provisioning (`terraform/module/workers/`).

### CI/CD

- GitHub Actions workflows: `vorpal.yaml` (main CI), `vorpal-nightly.yaml` (nightly builds).
- Renovate for automated dependency updates (`.github/renovate.json`).

### Self-Hosting

Vorpal builds itself. The `Vorpal.toml` at the repo root defines the build config for the `vorpal-config` binary, which uses the Rust SDK to build the CLI and related artifacts. The Go SDK config (`Vorpal.go.toml`) and TypeScript SDK config (`Vorpal.ts.toml`) define builds for the SDK packages themselves.

## Architectural Decisions

### Config-as-Code over DSL

Build configurations are real programs in general-purpose languages rather than YAML or a custom DSL. This provides full IDE support, type checking, conditional logic, and code reuse.

### Content-Addressed Storage

All artifacts and sources are identified by SHA-256 content digests. This ensures cache correctness -- identical inputs always resolve to the same cached output -- and enables safe sharing across machines and teams.

### Monolithic Service Process

Agent, Worker, and Registry run in a single process rather than as separate microservices. This simplifies deployment and local development while still maintaining clean service boundaries via gRPC interfaces. Services can be selectively enabled via the `--services` flag.

### gRPC for All Communication

All inter-component communication uses gRPC with Protocol Buffers. This provides:
- Strongly-typed contracts across all three SDK languages.
- Streaming support for large archive transfers.
- Built-in TLS and authentication interceptor support.

### Reproducibility via Timestamp Normalization

File timestamps are set to Unix epoch (1970-01-01) after all file operations. This ensures that content hashes are deterministic regardless of when builds occur, a key requirement for reproducible builds.

## Known Gaps

- **Git source type**: Declared in the agent's source type detection but returns a "not supported" error. Only local and HTTP sources are functional.
- **Post-build scripts**: A `TODO` in the build flow indicates planned but unimplemented post-build script execution.
- **Sandbox isolation**: Build steps run as subprocesses without containerization or filesystem isolation on the worker. The `temps` module creates sandbox directories but does not enforce process-level isolation. A `sandbox.go` file exists in the Go SDK's store package suggesting planned work.
- **Credential storage**: The login flow has a `TODO` noting that existing credentials should be loaded before overwriting.
- **Combined source digest**: A `TODO` in the agent notes exploring a combined source digest for the artifact rather than per-source digests.
