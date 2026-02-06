# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
# Setup development environment (installs rustup, protoc, terraform)
./script/dev.sh make build

# Common make targets
make build          # Build all crates (debug)
make check          # Type check
make format         # Check formatting (cargo fmt --all --check)
make lint           # Run clippy (--deny warnings)
make test           # Run tests
make clean          # Remove build artifacts

# Release builds
make build TARGET=release
make test TARGET=release

# Run Vorpal services locally
make vorpal-start   # Starts agent, registry, worker on https://localhost:23152

# Build using Vorpal (self-hosting)
make vorpal         # Builds the "vorpal" artifact

# Generate Go protobuf bindings from Rust proto definitions
make generate

# Vendored builds (offline)
make vendor
make .cargo         # Configure cargo to use vendored deps
```

## Architecture

Vorpal is a distributed build system with Rust and Go SDKs. Components communicate via gRPC (protobuf definitions in `sdk/rust/api/`).

### Workspace Structure

```
cli/                    # Main CLI binary (orchestrator)
config/                 # Configuration crate (vorpal-config)
sdk/
  rust/                 # vorpal-sdk crate
    api/                # Protobuf definitions (source of truth)
    src/
      artifact/         # Artifact builders (rust, go, linux, oci, tools)
      context.rs        # Build context management
  go/
    pkg/
      api/              # Generated Go protobuf bindings
      artifact/         # Go artifact builders (mirrors Rust SDK)
      config/           # Go context/config helpers
```

### Key Concepts

- **Artifact**: Hermetic, reproducible build output. Defined via `Artifact::new()` with steps and target systems.
- **ArtifactStep**: Individual build step with entrypoint, arguments, environment, and dependencies.
- **Context**: Build session that collects artifacts and runs them against services.
- **Systems**: Target platforms (`Aarch64Darwin`, `Aarch64Linux`, `X8664Darwin`, `X8664Linux`).

### Service Architecture

- **CLI**: Orchestrates builds, talks to services
- **Agent service**: Filesystem/sandbox tasks (localhost)
- **Registry service**: Persists artifacts (can be S3-backed)
- **Worker service**: Executes steps in isolated environments

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
vorpal run rsync --registry https://registry.example.com:23151
```

Resolution order: local alias file → registry lookup → error with build hint.

## Testing

```bash
cargo test                           # All tests
cargo test --package vorpal-sdk      # SDK tests only
cargo test test_name                 # Single test
```

## Protobuf Workflow

Proto files live in `sdk/rust/api/`. Rust bindings are generated at build time via `tonic-prost-build`. Go bindings must be regenerated manually:

```bash
make generate  # Regenerates sdk/go/pkg/api/ from sdk/rust/api/
```

## Rust Toolchain

Pinned to Rust 1.89.0 via `rust-toolchain.toml`. Components: clippy, rust-analyzer, rustfmt.

## Lima (Linux VM Development)

For testing Linux builds on macOS:

```bash
make lima              # Create and start VM
make lima-sync         # Sync source to VM
make lima-vorpal       # Build inside VM
make lima-vorpal-start # Run services inside VM
```
