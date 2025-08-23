# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Vorpal is a build and distribution platform that uses declarative configurations to build software distributively and natively across multiple platforms. It's written primarily in Rust with SDKs for multiple languages (Rust, Go, Python, TypeScript).

## Development Commands

### Core Build Commands
- `make` or `make build`: Build the Rust workspace (use `TARGET=release` for optimized builds)
- `make check`: Fast type-check via `cargo check`
- `make format`: Format code with `rustfmt`
- `make lint`: Lint with Clippy (warnings are treated as errors)
- `make test`: Run all Rust tests
- `make clean`: Clean build artifacts and generated files

### Development Environment
- `./script/dev.sh <command>`: Run commands in isolated development environment
- Example: `./script/dev.sh cargo build`
- Works with `direnv` for automatic environment setup

### Vorpal-Specific Commands
- `make vorpal-start`: Start local services on `localhost:23152` (required for registry/worker interactions)
- `make vorpal`: Build the repository using Vorpal itself (requires services running)
- `make vorpal-config-start`: Start config service for artifact configurations on port 50051
- `make dist`: Package binary in `./dist` directory as tarball

### Protocol Buffer Generation
- `make generate`: Regenerate Go API stubs from protobufs in `sdk/rust/api`

## Architecture

### Workspace Structure
- **`cli/`**: Main Rust CLI binary (`vorpal`) with entry point at `cli/src/main.rs`
- **`sdk/rust/`**: Rust SDK crate (`vorpal-sdk`) with protobuf APIs and artifact builders
- **`sdk/go/`**: Go SDK with generated protobuf bindings
- **`config/`**: Config-driven artifacts and tasks; binary `vorpal-config`
- **`script/`**: Development and CI helper scripts
- **`terraform/`**: Infrastructure as code for deployment

### Key Components
- **Artifacts**: Core abstraction for describing software builds with sources, steps, and target systems
- **Registry**: Service for storing and retrieving build artifacts
- **Worker**: Service for executing build steps
- **Agent**: Service for coordinating builds
- **Context**: Runtime environment for executing artifact builds

### Service Dependencies
Local services are required for registry/worker interactions. One-time setup:
1. `bash ./script/install.sh` (may require sudo)
2. `./target/debug/vorpal system keys generate`

Per development session: `make vorpal-start`

## Testing

- Tests use Rust `#[test]` and `#[tokio::test]` attributes
- Tests are colocated with code where practical
- Run tests with `make test`
- Run specific tests with `cargo test test_name`
- Name tests with `test_*` prefix and clear intent

## Code Style

- Rust 2021 edition
- Use `snake_case` for functions/variables, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for constants
- Run `cargo fmt` and ensure Clippy passes with no warnings
- Keep modules small and focused

## Development Workflow

1. Always run `make format lint test` before committing
2. For Vorpal-in-Vorpal builds, ensure services are running with `make vorpal-start`
3. Use `./script/dev.sh` for isolated development environment when needed
4. Use `direnv allow` for automatic environment setup with direnv
5. Follow commit message conventions: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`

## Additional Tools

- **Lima**: Use `make lima` to create isolated VM environment for testing
- **Vendor**: Use `make vendor` to create vendored dependencies for offline builds
- **Cleanup**: Use `make clean` to remove all build artifacts and generated files