# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Build and Development
- `make build` - Standard build using Cargo (default target)
- `make build TARGET=release` - Release build
- `cargo build` - Direct Cargo build (after setup)
- `./script/dev.sh cargo build` - Build in isolated development environment

### Code Quality
- `make lint` - Run Clippy with deny warnings (run before pushing)
- `make format` - Check code formatting with rustfmt
- `make check` - Run cargo check
- `make test` - Run test suite

### Development Environment Setup
- `./script/dev.sh` - Setup isolated development environment and run commands
- `direnv allow` - If using direnv for automatic environment management

### Vorpal-specific Commands
- `make vorpal-start` - Start Vorpal services locally on port 23152
- `make vorpal` - Build Vorpal using itself (requires services running)
- `./target/debug/vorpal system keys generate` - Generate keys (after initial build)
- `./target/debug/vorpal start` - Start Vorpal services
- `./target/debug/vorpal artifact make "vorpal"` - Build artifact using Vorpal

### Testing Full Stack
1. `make build` - Build without Vorpal
2. `bash ./script/install.sh` - Install (requires sudo)
3. `./target/debug/vorpal system keys generate` - Generate keys
4. `./target/debug/vorpal start` - Start services
5. `./target/debug/vorpal artifact make "vorpal"` - Build with Vorpal

### Protocol Buffers
- `make generate` - Regenerate Go gRPC bindings from Rust protobuf definitions

## Architecture

Vorpal is a distributed build system with a multi-language SDK approach:

### Core Components
- **CLI (`cli/`)** - Main Vorpal binary (`vorpal-cli` → `vorpal` executable)
- **Config Service (`config/`)** - Configuration management service (`vorpal-config`)
- **Rust SDK (`sdk/rust/`)** - Core SDK library (`vorpal-sdk`)
- **Go SDK (`sdk/go/`)** - Go language bindings with generated gRPC clients

### Key Architecture Patterns
- **Workspace Structure**: Cargo workspace with `cli`, `config`, and `sdk/rust` members
- **gRPC Services**: Agent, Archive, Artifact, Context, and Worker services
- **Multi-target Builds**: Supports aarch64/x86_64 on Darwin/Linux
- **Artifact System**: Declarative configuration using `Artifact` structures with sources, steps, and target systems
- **SDK Pattern**: Language-specific builders (e.g., `RustArtifactBuilder`) wrap core artifact definitions

### Service Architecture
- **Registry**: Artifact and archive management (default port 23152)
- **Agent**: Distributed build coordination
- **Worker**: Build execution
- **Context**: Build context management

### Configuration Files
- `Vorpal.toml` - Project-level Vorpal configuration
- `Vorpal.go.toml` - Go SDK specific configuration
- Individual language templates in `cli/src/command/template/`

### Development Environment
- Uses `./script/dev.sh` for isolated dependency management
- Supports direnv for automatic environment setup
- Cross-platform development with Lima VM support for Linux testing
- Rust toolchain pinned to 1.89.0 with clippy, rust-analyzer, rustfmt

The system enables "bring-your-own-language" build configurations where developers can use Rust, Go, Python, or TypeScript SDKs to define the same underlying artifact build processes.