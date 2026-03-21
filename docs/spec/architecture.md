---
project: "vorpal"
maturity: experimental
last_updated: "2026-03-20"
updated_by: "@staff-engineer"
scope: "System architecture, component boundaries, and dependency graph"
owner: "@staff-engineer"
dependencies:
  - security.md
  - performance.md
---

# Architecture

## Overview

Vorpal is a build system that treats build configurations as real programs. Users define builds in Rust, Go, or TypeScript using language-specific SDKs. Vorpal provides hermetic execution, cross-platform targeting (macOS Apple Silicon/Intel, Linux x86_64/ARM64), content-addressed caching, and artifact distribution through a gRPC-based service architecture.

## Repository Structure

```
.
├── cli/                  # Rust CLI binary ("vorpal")
│   └── src/
│       ├── main.rs       # Entry point
│       └── command/      # Subcommand implementations
│           ├── build.rs  # Build orchestration
│           ├── init.rs   # Project scaffolding
│           ├── run.rs    # Artifact execution
│           ├── start/    # Service server (agent, registry, worker)
│           ├── store/    # Local store (archives, hashes, notary, paths, temps)
│           └── template/ # Project templates (Go, Rust, TypeScript)
├── config/               # Self-hosted build configuration (vorpal-config)
├── sdk/
│   ├── rust/             # Rust SDK (vorpal-sdk crate, published to crates.io)
│   │   ├── api/          # Protobuf definitions (source of truth)
│   │   └── src/
│   │       └── artifact/ # Artifact builders (language-specific, tools, OS images)
│   ├── go/               # Go SDK (generated from protos + hand-written helpers)
│   │   └── pkg/
│   │       ├── api/      # Generated gRPC/protobuf code
│   │       ├── artifact/ # Artifact builders (language-specific)
│   │       ├── config/   # Context and auth helpers
│   │       └── store/    # Store utilities
│   └── typescript/       # TypeScript SDK (@altf4llc/vorpal-sdk, published to npm)
│       └── src/
│           ├── api/      # Generated ts-proto code
│           └── artifact/ # Artifact builders
├── script/               # Dev and CI scripts
├── terraform/            # Keycloak IDP configuration
└── docs/                 # Documentation (TDDs, specs)
```

## System Components

### CLI (`cli/`)

The `vorpal` binary is the primary user interface. Written in Rust with `clap` for argument parsing and `tokio` for async runtime. Key subcommands:

- **`build`** -- Compiles a user's build config (SDK program), connects to the agent service, and orchestrates artifact preparation and execution.
- **`run`** -- Fetches and executes a previously built artifact from the registry by alias.
- **`init`** -- Scaffolds a new project with a `Vorpal.toml` and template build config.
- **`system services start`** -- Starts the gRPC server hosting agent, registry, and worker services.
- **`system keys generate`** -- Generates TLS key pairs for service communication.
- **`system prune`** -- Cleans up local store artifacts, archives, configs, outputs, and sandboxes.
- **`login`** -- OAuth2 Device Authorization Grant flow for OIDC authentication.
- **`inspect`** -- Retrieves artifact metadata from the registry by digest.
- **`config`** -- Manages layered configuration (user-level + project-level + defaults).

### SDKs (`sdk/`)

Three SDKs provide the "config as code" experience. Each SDK connects to the Vorpal services via gRPC and produces artifact definitions.

- **Rust SDK** (`vorpal-sdk`): The canonical SDK. Contains the protobuf definitions in `sdk/rust/api/` which are the source of truth. Published to crates.io as `vorpal-sdk`.
- **Go SDK**: Generated gRPC stubs from the same protos via `protoc`. Hand-written artifact builders in `pkg/artifact/`. Module: `github.com/ALT-F4-LLC/vorpal/sdk/go`.
- **TypeScript SDK**: Generated via `ts-proto` from the same protos. Published to npm as `@altf4llc/vorpal-sdk`.

All three SDKs must produce identical artifact digests for the same inputs -- this is verified in CI via cross-SDK parity tests.

### Services (inside `cli/src/command/start/`)

Vorpal runs a set of gRPC services, all hosted in a single binary process:

- **Agent Service** -- Accepts `PrepareArtifact` requests. Resolves sources (local, HTTP, git), computes content-addressed digests, pushes source archives to the registry, encrypts secrets, and returns prepared artifact definitions. Maintains an in-memory source cache per session.
- **Archive Service** (Registry) -- Stores and retrieves compressed source archives. Supports local filesystem and S3 backends. Content-addressed by digest. Includes a TTL-based cache for archive existence checks.
- **Artifact Service** (Registry) -- Stores and retrieves artifact metadata (protobuf-serialized). Supports alias lookups (namespace/name:tag) for `vorpal run`.
- **Worker Service** -- Executes build steps. Receives artifact definitions, pulls source archives, runs step scripts in sandboxed environments, and pushes outputs. Supports pluggable executors (bash, docker, bubblewrap).

Transport: Unix Domain Socket (default) or TCP with optional TLS. A separate plaintext health check endpoint can be enabled.

### Protobuf API (`sdk/rust/api/`)

Five proto packages define the service contracts:

| Package | Services | Purpose |
|---------|----------|---------|
| `agent` | `AgentService` | Artifact preparation |
| `archive` | `ArchiveService` | Source archive storage |
| `artifact` | `ArtifactService` | Artifact metadata storage |
| `context` | `ContextService` | Build context management |
| `worker` | `WorkerService` | Build step execution |

Proto definitions live in `sdk/rust/api/` and are compiled to:
- Rust: via `tonic-prost-build` at build time (`sdk/rust/build.rs`)
- Go: via `protoc` + `protoc-gen-go` / `protoc-gen-go-grpc` (`make generate`)
- TypeScript: via `protoc` + `ts-proto` (`make generate`)

### Build Configuration (`Vorpal.toml`)

A TOML configuration file that specifies which SDK language to use, project name, source includes, and language-specific options. Supports layered configuration: user-level (`~/.vorpal/settings.json`) + project-level (`Vorpal.toml`) + CLI flags.

### Local Store (`cli/src/command/store/`)

On-disk content-addressed storage at `/var/lib/vorpal/`. Organized into:
- **archives** -- Compressed source tarballs
- **configs** -- Serialized artifact definitions
- **outputs** -- Build step outputs
- **sandboxes** -- Temporary build environments
- **keys** -- TLS certificates and private keys

### Lockfile (`Vorpal.lock`)

JSON lockfile recording source digests per platform. Prevents rebuild drift for HTTP sources. Updated automatically during builds. Supports `--unlock` flag for intentional updates.

## Dependency Graph

```
User SDK Program
    │
    ▼
SDK (Rust/Go/TS) ──gRPC──► Agent Service
                               │
                    ┌──────────┼──────────┐
                    ▼          ▼          ▼
              Archive Svc  Artifact Svc  Worker Svc
              (S3/local)   (S3/local)   (bash/docker/bwrap)
```

## Key Design Decisions

1. **Protobuf as interface contract** -- All service boundaries are defined via protobuf. The Rust SDK owns the proto definitions; Go and TypeScript SDKs are generated from them.

2. **Content-addressed everything** -- Artifacts and sources are identified by SHA-256 digests. Same inputs always produce the same digest. This enables caching and deduplication.

3. **Single binary** -- The CLI and all services ship as one binary (`vorpal`). Services are enabled/disabled via the `--services` flag. This simplifies deployment.

4. **SDK parity** -- The CI pipeline verifies that all three SDKs produce byte-identical artifact digests for the same build configuration. This ensures language choice is purely a user preference.

5. **Pluggable executors** -- Build steps default to bash but can use docker, bubblewrap, or any binary as the entrypoint. This allows hermetic execution without mandating containerization.

6. **Unix Domain Sockets by default** -- Local development uses UDS for zero-configuration networking. TCP with TLS is available for remote/production setups.

## Gaps and Limitations

- Git source type is defined but not yet implemented (`bail!("git not supported")`).
- No formal API versioning strategy for the protobuf services.
- The `context` proto package exists but its service integration is unclear from the codebase.
- No explicit module boundary enforcement between CLI internals and SDK -- they share the same workspace.
