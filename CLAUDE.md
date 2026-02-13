# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Vorpal

Vorpal is a distributed, language-agnostic artifact build and execution system. It uses gRPC services (agent, registry, worker) to build, store, and run artifacts across multiple platforms (aarch64-darwin, aarch64-linux, x86_64-darwin, x86_64-linux). Services run in plaintext mode by default; mutual TLS is available via the `--tls` flag for production deployments.

## Build & Development Commands

```bash
# Setup development environment (installs rustup, protoc, terraform)
./script/dev.sh make build

make build          # Build all workspace crates (debug)
make build TARGET=release  # Build release (offline, requires vendored deps)
make check          # cargo check
make format         # cargo fmt --all --check
make lint           # cargo clippy -- --deny warnings
make test           # cargo test
make clean          # Remove build artifacts
make vendor         # Vendor Cargo dependencies for offline builds
make .cargo         # Configure cargo to use vendored deps
make generate       # Regenerate Go protobuf code from proto files
```

Running a single test:
```bash
cargo test -p <crate-name> <test_name>
```

Workspace crates: `vorpal-cli`, `vorpal-sdk`, `vorpal-config`

### Running Vorpal Locally

```bash
# 1. Start all services (agent + registry + worker on port 23153, plaintext by default)
make vorpal-start

# 2. Build an artifact
make vorpal                              # builds default (vorpal)
make VORPAL_ARTIFACT=vorpal-shell vorpal # builds specific artifact

# Optional: enable TLS (requires generating keys first)
cargo run --bin vorpal -- system keys generate
make vorpal-start VORPAL_FLAGS="--tls"
```

### Go SDK

```bash
cd sdk/go
go test ./...
go build ./...
```

## Key Concepts

- **Artifact**: Hermetic, reproducible build output. Defined via `Artifact::new()` with steps and target systems.
- **ArtifactStep**: Individual build step with entrypoint, arguments, environment, and dependencies.
- **Context**: Build session that collects artifacts and runs them against services.
- **Systems**: Target platforms (`Aarch64Darwin`, `Aarch64Linux`, `X8664Darwin`, `X8664Linux`).

## Architecture

### Workspace Structure

- **`cli/`** (`vorpal-cli`) - CLI binary (`vorpal`). Orchestrates builds, runs artifacts, manages services and TLS keys. Entry point: `cli/src/main.rs`, commands in `cli/src/command/`.
- **`sdk/rust/`** (`vorpal-sdk`) - Rust SDK. Contains protobuf definitions (`sdk/rust/api/*.proto`), artifact builders, and `ConfigContext` for service communication.
- **`sdk/go/`** - Go SDK. Mirrors the Rust SDK. Generated protobuf code in `sdk/go/pkg/api/`, artifact builders in `sdk/go/pkg/artifact/`.
- **`config/`** (`vorpal-config`) - Vorpal's own build configuration. Defines artifacts like `vorpal`, `vorpal-shell`, `vorpal-release`, `vorpal-container-image`.

### Service Architecture

All three services run on the same port (default 23153). By default, services use plaintext (HTTP); pass `--tls` to enable mutual TLS:

1. **Agent** (`AgentService::PrepareArtifact`) - Prepares/stages artifacts locally, manages sandboxes
2. **Registry** (`ArtifactService` + `ArchiveService`) - Stores artifact metadata and binary archives (local or S3 backend). Archives use zstd compression with chunked streaming.
3. **Worker** (`WorkerService::BuildArtifact`) - Executes build steps in isolated environments, pushes results to registry

Service implementations: `cli/src/command/start/` (agent.rs, registry.rs, worker.rs)

### SDK Patterns

Both Rust and Go SDKs follow the same builder pattern:

```rust
// Rust SDK
Artifact::new("name", steps, systems).build(ctx).await?;
```

```go
// Go SDK
artifact.NewArtifact("name", steps, systems).Build(ctx)
```

Pre-built artifact helpers exist for common tools (crane, gh, git, protoc, rust-toolchain, etc.) and language builds (Rust, Go).

### Build Flow

CLI reads `Vorpal.toml` → connects to Agent (prepare) → Worker (build steps) → Registry (store archive + metadata) → returns digest

### Protobuf Definitions

Source of truth: `sdk/rust/api/` with five proto files (agent, archive, artifact, context, worker). Rust code is generated at build time via `sdk/rust/build.rs` using tonic-prost. Go code is generated via `make generate` using protoc.

### Configuration Files

- `Vorpal.toml` - Rust artifact build config
- `Vorpal.go.toml` - Go artifact build config (used for SDK parity testing)
- `rust-toolchain.toml` - Pinned to Rust 1.89.0

### TLS

TLS is optional. By default, services start in plaintext mode (HTTP) for local development. To enable TLS, pass `--tls` to `vorpal system services start` and use `https://` addresses for client connections. TLS keys/certs are managed via `vorpal system keys generate` and stored in `/var/lib/vorpal/`. The shared TLS client config helper is in the SDK (`get_client_tls_config()` in Rust, `getTransportCredentials()` in Go). Both SDKs auto-detect the transport based on the address scheme (`http://` = plaintext, `https://` = TLS). CA certificate is optional when using TLS (falls back to system trust store).

### Artifact Store

Artifacts are stored under `/var/lib/vorpal/` with archives, hashes, sandboxes, and notary data managed by the store module (`cli/src/command/store/`).

## Running Artifacts

After building, use `vorpal run` to execute artifacts directly from the store:

```bash
# Run a built artifact (uses local store, falls back to registry)
vorpal run <alias> [-- <args>...]

# Alias format: [<namespace>/]<name>[:<tag>]
vorpal run rsync -- --help              # name only (namespace=library, tag=latest)
vorpal run rsync:3.4.1 -- -avz src/ dst/  # name with tag
vorpal run team/my-tool:v2.0            # namespace, name, and tag

# Override which binary to execute (default: artifact name)
vorpal run my-tool --bin my-tool-helper -- --verbose

# Use a specific registry
vorpal run rsync --registry http://registry.example.com:23151
```

Resolution order: local alias file → registry lookup → error with build hint.

## Lima (Linux VM Development)

For testing Linux builds on macOS:

```bash
make lima              # Create and start VM
make lima-sync         # Sync source to VM
make lima-vorpal       # Build inside VM
make lima-vorpal-start # Run services inside VM
```
