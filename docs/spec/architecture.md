---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "System architecture, component boundaries, data flow, and integration points for the Vorpal build system"
owner: "@staff-engineer"
dependencies:
  - security.md
  - operations.md
  - performance.md
---

# Architecture Specification

## 1. System Overview

Vorpal is a cross-platform build system where build configurations are written as real programs (not YAML or DSLs) using SDKs in Rust, Go, or TypeScript. The system provides hermetic execution, content-addressed caching, and artifact distribution through a client-server architecture communicating over gRPC.

### Design Philosophy

- **Config as code**: Build definitions are executable programs compiled and run by the CLI, not declarative configuration files.
- **Content-addressed**: Artifacts are identified by SHA-256 digests of their serialized JSON representation, enabling deterministic caching and deduplication.
- **Multi-language SDK parity**: Rust, Go, and TypeScript SDKs expose equivalent APIs; CI verifies that all three produce identical artifact digests for the same inputs.
- **Pluggable executors**: Build steps run via `bash` (macOS), `bwrap` (Linux sandboxed), `docker`, or arbitrary binaries.

### Target Platforms

| Platform | Architecture | Status |
|----------|-------------|--------|
| macOS | Apple Silicon (aarch64) | Supported |
| macOS | Intel (x86_64) | Supported |
| Linux | ARM64 (aarch64) | Supported |
| Linux | x86_64 | Supported |

Defined as the `ArtifactSystem` protobuf enum with values: `AARCH64_DARWIN`, `AARCH64_LINUX`, `X8664_DARWIN`, `X8664_LINUX`.

## 2. Repository Structure

```
vorpal/
  cli/                    # Rust CLI binary (vorpal-cli crate, binary name: "vorpal")
    src/
      command/
        build.rs          # Build orchestration: config compilation + artifact building
        config.rs         # Vorpal.toml parsing, layered settings resolution
        config_cmd.rs     # `vorpal config` subcommand (get/set/show)
        init.rs           # `vorpal init` project scaffolding
        inspect.rs        # `vorpal inspect` artifact inspection
        lock.rs           # Vorpal.lock lockfile management
        run.rs            # `vorpal run` artifact execution
        start/            # Service startup
          agent.rs        # Agent gRPC service implementation
          auth.rs         # OIDC token validation, namespace authorization
          registry.rs     # Archive + Artifact registry services
          worker.rs       # Worker gRPC service (build execution)
        store/            # Local store operations
          archives.rs     # zstd compression/decompression
          hashes.rs       # Source digest computation
          notary.rs       # RSA encrypt/decrypt for secrets
          paths.rs        # Filesystem path conventions (/var/lib/vorpal/...)
          temps.rs        # Sandbox directory management
        system/           # System management (keys, prune)
  config/                 # Vorpal's own build config (vorpal-config crate)
    src/
      artifact/           # Self-referential artifacts (vorpal builds itself)
  sdk/
    rust/                 # Rust SDK (vorpal-sdk crate, published to crates.io)
      api/                # Protobuf definitions (source of truth)
        agent/agent.proto
        archive/archive.proto
        artifact/artifact.proto
        context/context.proto
        worker/worker.proto
      src/
        artifact/         # Artifact builders (language, tool, system)
        context.rs        # ConfigContext, gRPC channel builder, credentials
        cli.rs            # SDK-side CLI argument parsing for config binaries
    go/                   # Go SDK
      pkg/
        api/              # Generated protobuf code (from sdk/rust/api/)
        artifact/         # Artifact builders
        config/           # ConfigContext equivalent
        store/            # Hash, path, sandbox utilities
      cmd/vorpal/         # Vorpal's own Go config entry point
    typescript/           # TypeScript SDK (published to npm as @altf4llc/vorpal-sdk)
      src/
        api/              # Generated protobuf code (from sdk/rust/api/)
        artifact/         # Artifact builders
        context.ts        # ConfigContext equivalent
  terraform/              # AWS infrastructure for dev/CI workers
  script/                 # Shell scripts (install, dev environment setup)
  docs/
    tdd/                  # Technical design documents
    spec/                 # Project specifications (this directory)
```

### Cargo Workspace

The Rust workspace (`Cargo.toml`) contains three members:

| Crate | Type | Published | Description |
|-------|------|-----------|-------------|
| `vorpal-cli` (cli/) | Binary | No | CLI binary, all server implementations |
| `vorpal-config` (config/) | Binary | No | Vorpal's own build configuration |
| `vorpal-sdk` (sdk/rust/) | Library | crates.io | Public SDK for writing build configs |

## 3. Component Architecture

### 3.1 CLI (`vorpal`)

The single `vorpal` binary serves as both the client-side build tool and the server daemon. Key subcommands:

| Command | Role |
|---------|------|
| `vorpal build <name>` | Compile config, resolve dependencies, execute builds |
| `vorpal run <alias>` | Execute a built artifact from the store |
| `vorpal init <name>` | Scaffold a new project |
| `vorpal inspect <digest>` | Inspect artifact metadata in the registry |
| `vorpal login` | OAuth2 device code flow for registry authentication |
| `vorpal config` | Get/set/show layered configuration |
| `vorpal system services start` | Start background services (agent, registry, worker) |
| `vorpal system keys generate` | Generate TLS CA + service certificates |
| `vorpal system prune` | Clean up local store (aliases, archives, configs, outputs, sandboxes) |

#### Configuration Layering

Settings are resolved in priority order (highest wins):

1. CLI flags (explicit `--flag` values)
2. Project config (`Vorpal.toml` in working directory)
3. User config (`~/.vorpal/settings.json`)
4. Built-in defaults

The `Vorpal.toml` file specifies:

```toml
language = "rust"       # go | rust | typescript
name = "vorpal-config"  # config binary name

[source]
includes = ["config", "sdk/rust"]

[source.rust]
packages = ["vorpal-config", "vorpal-sdk"]
```

### 3.2 Services

All services run within a single process started by `vorpal system services start`. They share a single gRPC server (either Unix domain socket or TCP with optional TLS).

**Selectable services** via `--services` flag (default: `agent,registry,worker`):

#### Agent Service

**Proto**: `vorpal.agent.AgentService`
**RPC**: `PrepareArtifact(PrepareArtifactRequest) -> stream PrepareArtifactResponse`

Responsibilities:
- Receives raw artifact definitions from config binaries
- Resolves artifact sources (local files, HTTP downloads, git -- git not yet implemented)
- Computes content-addressed digests for sources
- Encrypts secrets using RSA public key
- Pushes source archives to the registry
- Manages `Vorpal.lock` lockfile for HTTP sources
- Caches HTTP source digests within a session (avoids re-downloading identical URLs)

Source types supported:
- **Local**: Files from the project context directory
- **HTTP/HTTPS**: Downloaded, auto-detected by MIME type (gzip, bzip2, xz, zip, binaries), unpacked into sandbox
- **Git**: Declared in code but currently returns an error ("git not supported")

#### Registry Service

**Protos**: `vorpal.archive.ArchiveService`, `vorpal.artifact.ArtifactService`

Two sub-services share the "registry" role:

**ArchiveService** RPCs:
- `Check(ArchivePullRequest) -> ArchiveResponse` -- check if archive exists
- `Pull(ArchivePullRequest) -> stream ArchivePullResponse` -- download archive
- `Push(stream ArchivePushRequest) -> ArchiveResponse` -- upload archive

**ArtifactService** RPCs:
- `GetArtifact(ArtifactRequest) -> Artifact` -- fetch artifact metadata by digest
- `GetArtifactAlias(GetArtifactAliasRequest) -> GetArtifactAliasResponse` -- resolve alias to digest
- `GetArtifacts(ArtifactsRequest) -> ArtifactsResponse` -- list artifact digests
- `StoreArtifact(StoreArtifactRequest) -> ArtifactResponse` -- store artifact with aliases

**Storage backends**:
- `local`: Filesystem at `/var/lib/vorpal/store/` (default)
- `s3`: AWS S3 bucket (configured via `--registry-backend-s3-bucket`)

Archive caching: `moka` in-memory cache with configurable TTL (default 300s) for `Check` results to avoid repeated S3 HEAD requests.

#### Worker Service

**Proto**: `vorpal.worker.WorkerService`
**RPC**: `BuildArtifact(BuildArtifactRequest) -> stream BuildArtifactResponse`

Responsibilities:
- Receives artifact build requests with fully resolved sources
- Validates target system matches the worker's host system
- Pulls sources and dependency artifacts from the registry
- Creates sandboxed workspace directories
- Executes build steps sequentially (process spawning with stdout/stderr streaming)
- Packs output into zstd-compressed tar archives
- Pushes built archives back to the registry
- Stores artifact metadata in the registry
- Manages artifact output lock files to prevent concurrent builds

#### Context Service

**Proto**: `vorpal.context.ContextService`
**RPCs**: `GetArtifact`, `GetArtifacts`

A short-lived gRPC server spawned by the CLI during `vorpal build`. It runs the compiled config binary as a child process, which connects back to this service to register its artifact definitions. After the config binary exits, the CLI reads the registered artifacts and proceeds with building.

### 3.3 SDKs

All three SDKs expose equivalent APIs for defining artifacts programmatically. The canonical protobuf definitions live in `sdk/rust/api/` and are generated for Go and TypeScript via `make generate` using `protoc`.

#### SDK Public API Surface

Each SDK provides:

- **ConfigContext**: Connection to agent + artifact services, artifact store, variable resolution
- **Artifact builders**: `Artifact`, `ArtifactSource`, `ArtifactStep`, `Job`, `Process`, `OciImage`
- **Language builders**: `Go`, `Rust`, `TypeScript` (plus corresponding `DevelopmentEnvironment` variants)
- **Environment builders**: `DevelopmentEnvironment`, `UserEnvironment`
- **Step functions**: `bash()`, `bwrap()`, `shell()`, `docker()` -- platform-aware step construction
- **Tool artifacts**: Pre-built toolchains (bun, cargo, crane, gh, git, go, nodejs, protoc, rsync, rust-toolchain, staticcheck, etc.)

#### Config Binary Lifecycle

1. CLI compiles the config source into a binary using the appropriate toolchain
2. CLI starts a `ContextService` gRPC server on a random port
3. CLI spawns the config binary, which connects to the `ContextService`
4. Config binary creates artifacts via `context.add_artifact()`, which calls the Agent to prepare sources
5. Config binary exits; CLI queries `ContextService.GetArtifacts()` to retrieve the full artifact graph
6. CLI builds artifacts in dependency order via the Worker

## 4. Data Flow

### Build Pipeline

```
User runs: vorpal build <name>
    |
    v
1. Parse Vorpal.toml (language, source config)
2. Compile config binary (using language-specific builder)
3. Build config dependencies (sdk toolchain, protoc, etc.)
4. Start ContextService, spawn config binary
5. Config binary registers artifacts via Agent
    |
    v
Agent: PrepareArtifact
    - Resolve sources (local/HTTP)
    - Compute source digests
    - Encrypt secrets
    - Push source archives to registry
    - Return artifact with resolved digests
    |
    v
6. CLI reads artifact graph from ContextService
7. Topological sort (petgraph) for build order
8. For each artifact (in dependency order):
    |
    v
    a. Check local store for existing output
    b. Try pulling from registry
    c. If not found: Worker.BuildArtifact
        - Pull sources from registry
        - Pull dependency artifacts
        - Execute steps (bash/bwrap/docker)
        - Pack output, push to registry
    d. Pull built artifact to local store
    |
    v
9. Print artifact digest (or path with --path)
```

### Content Addressing

Artifacts are identified by SHA-256 of their JSON-serialized `Artifact` protobuf message. This means the digest incorporates:
- Target system
- All source definitions (including source digests)
- All build steps (entrypoints, scripts, arguments, environments, dependency digests)
- Artifact name and aliases

Source digests are computed by hashing all file contents in sorted order after timestamp normalization.

### Local Store Layout

```
/var/lib/vorpal/
  key/
    ca.pem              # CA certificate
    ca.key.pem          # CA private key
    service.pem         # Service certificate
    service.key.pem     # Service private key
    service.public.pem  # Service public key (for secret encryption)
    credentials.json    # OAuth2 credentials
  sandbox/              # Temporary build workspaces
  store/
    <namespace>/
      archive/
        <digest>.tar.zst  # Compressed archives (sources and built artifacts)
      config/
        <digest>.json     # Artifact metadata
      output/
        <digest>/         # Unpacked artifact outputs
          bin/            # Executables
          ...
      alias/
        <name>-<system>-<tag>  # Alias -> digest mapping
  vorpal.sock           # Unix domain socket (default)
  vorpal.lock           # Advisory lock file
```

## 5. Communication Protocols

### gRPC Transport

- **Local mode** (default): Unix domain socket at `/var/lib/vorpal/vorpal.sock` (overridable via `VORPAL_SOCKET_PATH` env var)
- **TCP mode**: `--port <port>` flag enables TCP listener on `[::]:<port>`
- **TLS mode**: `--tls` flag enables TLS on TCP (default port 23151), requires generated certificates
- **Health checks**: Optional plaintext TCP health endpoint on port 23152 (`--health-check` flag), uses `tonic-health`

Transport selection in SDK `build_channel()`:
- `unix://` prefix -> Unix domain socket via tower connector
- `http://` prefix -> plaintext TCP
- `https://` prefix -> TLS TCP (uses CA cert from `/var/lib/vorpal/key/ca.pem` or native roots)

### Streaming Patterns

- Agent `PrepareArtifact`: Server-streaming (progress messages + final artifact)
- Worker `BuildArtifact`: Server-streaming (build output lines + completion)
- Archive `Pull`: Server-streaming (chunked binary data, 2MB chunks for S3, 8KB for push)
- Archive `Push`: Client-streaming (chunked binary data, 8KB chunks)

## 6. Build Isolation

### macOS

Build steps execute directly via `bash` with controlled environment variables. No filesystem sandboxing. The `PATH` is constructed from artifact `bin/` directories plus system defaults.

### Linux

Build steps execute inside `bwrap` (Bubblewrap) containers with:
- `--unshare-all --share-net`: Namespace isolation with network access
- `--clearenv`: Clean environment
- `--chdir $VORPAL_WORKSPACE`: Working directory set to sandbox
- `--gid 1000 --uid 1000`: Non-root user
- `--dev /dev`, `--proc /proc`, `--tmpfs /tmp`: Minimal filesystem
- `--ro-bind` for rootfs (from `LinuxVorpal` artifact) and dependency artifacts
- `--bind` for output and workspace directories

The rootfs is a custom Linux distribution built from source (`linux_vorpal/` module with multi-stage build scripts). A slim variant (`linux_vorpal_slim`) is also available.

### Docker Executor

An alternative executor that delegates to `docker run` with volume mounts for `$VORPAL_OUTPUT`.

## 7. Dependency Management

### Artifact Dependencies

Artifacts declare step-level dependencies via `step.artifacts[]` (list of digest strings). These form a DAG resolved via `petgraph` topological sort. The CLI builds dependencies before dependents.

### Lockfile (`Vorpal.lock`)

TOML format tracking source digests by `(name, platform)`:

```toml
lockfile = 1

[[sources]]
digest = "sha256:..."
excludes = []
includes = ["..."]
name = "source-name"
path = "https://..."
platform = "aarch64-darwin"
```

The agent updates the lockfile incrementally for HTTP sources after each source is prepared. Local sources are not locked. The `--unlock` flag allows source digest changes.

### Vorpal Configuration Files

Multiple config files can coexist in a project:
- `Vorpal.toml` -- default (Rust config)
- `Vorpal.go.toml` -- Go config
- `Vorpal.ts.toml` -- TypeScript config

Each specifies `language`, `name`, `[source]` includes, and language-specific build settings.

## 8. Authentication and Authorization

### OIDC Integration

- **CLI login**: OAuth2 Device Code flow (`vorpal login --issuer <url>`)
- **Server validation**: OIDC JWT validation via JWKS endpoint discovery
- **Token refresh**: Automatic refresh using stored refresh tokens (5-minute expiry buffer)
- **Service-to-service**: Worker uses OAuth2 Client Credentials flow for registry access

### Namespace Authorization

When auth is enabled (`--issuer` flag on service start):
- Registry and Worker services apply OIDC interceptors to validate JWT tokens
- Namespace permissions are enforced via JWT claims (`resource_access.<client>.roles`)
- Keycloak is the reference IdP (Terraform modules provision realm, clients, roles, scopes)

### Secret Management

Build step secrets are encrypted with the service RSA public key by the Agent and decrypted with the private key by the Worker at execution time. Secrets are passed as environment variables to build steps.

## 9. Infrastructure

### Terraform (Development)

The `terraform/` directory provisions AWS resources for development and CI:
- **VPC**: Single-AZ with public subnet
- **Registry instance**: `t4g.large` (ARM64 Ubuntu 24.04)
- **Worker instances**: Per-platform EC2 instances
  - `aarch64-linux`: `t4g.large`
  - `x8664-linux`: `t3a.large`
  - `aarch64-darwin`: `mac2.metal` (optional, dedicated host)
  - `x8664-darwin`: `mac1.metal` (optional, dedicated host)
- **Keycloak module**: Identity provider configuration for auth testing

### CI Pipeline (GitHub Actions)

Workflow: `.github/workflows/vorpal.yaml`

Stages:
1. **vendor**: Restore caches, vendor dependencies, cargo check (4 runners: macOS x2, Ubuntu x2)
2. **code-quality**: Format check (`cargo fmt --check`), lint (`cargo clippy --deny warnings`)
3. **build**: Release build, verify no non-system dynamic deps, test, dist (4 runners)
4. **test**: Integration test -- build artifacts using all three SDKs, verify digest parity across Rust/Go/TypeScript
5. **release**: Tag-triggered GitHub release with build provenance attestation
6. **release-container-image**: Docker image build and push to Docker Hub (multi-arch manifest)
7. **release-sdk-rust**: Publish `vorpal-sdk` to crates.io
8. **release-sdk-typescript**: Publish `@altf4llc/vorpal-sdk` to npm

## 10. Known Gaps and Limitations

- **Git source type**: Declared in code but returns an error at runtime ("git not supported")
- **No parallel artifact preparation**: `ConfigContext.add_artifact()` processes artifacts sequentially (marked with TODO)
- **No combined source digest**: Artifact digest does not incorporate a combined sources hash (marked with TODO)
- **No incremental builds**: Changing any source or step invalidates the entire artifact
- **Single worker per platform**: No work distribution or queue -- one worker handles all builds for its platform
- **No Windows support**: Platform enum and all tooling target only macOS and Linux
- **Local-only sandboxing on macOS**: No filesystem isolation on macOS (bash only, no bwrap equivalent)
- **Credentials file management**: `vorpal login` overwrites the entire credentials file rather than merging (marked with TODO)
- **No artifact garbage collection policy**: `vorpal system prune` is manual; no automatic eviction
- **Lockfile race conditions**: Lockfile updates during parallel builds could conflict (currently mitigated by sequential processing)
