# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Vorpal is a language-agnostic, declarative, reproducible build system. Users define build artifacts using Rust or Go SDKs, and Vorpal executes them hermetically across macOS and Linux (x86_64 and aarch64).

## Architecture

**Rust workspace with 3 crates:**
- `cli` — Main binary (`vorpal`). Orchestrates builds, manages local store, runs gRPC services, and provides the agent TUI.
- `config` — Helper binary for build configuration.
- `sdk/rust` — Rust SDK library. Contains protobuf definitions (source of truth in `sdk/rust/api/`), gRPC client/server code, and the artifact builder API.

**Go SDK** (`sdk/go/`) — Mirrors the Rust SDK. Generated from the same proto files. Used for SDK parity testing (Go and Rust builds must produce identical artifact digests).

**gRPC services** (all defined in `sdk/rust/api/`):
- **Agent** — Filesystem/sandbox preparation (streaming)
- **Registry** — Artifact and archive storage (local or S3-backed)
- **Worker** — Step execution in isolated environments

Services communicate over gRPC with TLS. The CLI can run all three in a single process for local development.

**Artifact system:** TOML-based config (`Vorpal.toml`), content-addressed store keyed by SHA256, supports aliases (`namespace/name:tag`).

## Build Commands

```bash
make build              # cargo build (debug)
make build TARGET=release  # cargo build --offline --release
make check              # cargo check
make format             # cargo fmt --all --check
make lint               # cargo clippy -- --deny warnings
make test               # cargo test
make generate           # regenerate Go proto code from sdk/rust/api/
make vorpal-start       # start all services locally (agent+registry+worker)
make vorpal             # build vorpal using itself (self-hosting)
```

Pre-commit quality gates: `make format && make lint && make test`

## Development Environment

- **Rust toolchain:** Pinned to 1.89.0 via `rust-toolchain.toml`
- **direnv:** `.envrc` runs `script/dev.sh` which installs system deps, rustup, protoc, and terraform into `.env/bin`
- **Lima:** `make lima` creates a Linux VM on macOS for cross-platform testing
- **Proto compilation:** Rust protos auto-compile via `sdk/rust/build.rs` (tonic-prost-build). Go protos require `make generate`.

## CI/CD

GitHub Actions (`.github/workflows/vorpal.yaml`):
1. Vendor + cargo check on all 4 platforms
2. Format + clippy lint (macOS)
3. Release builds on all 4 platforms (macOS x2, Linux x2)
4. Integration tests: builds vorpal with both Rust and Go SDKs, verifies artifact digest parity
5. On tag push: multi-arch Docker image build + GitHub release with attestations

## Issue Tracking

All planning and issue management uses Linear. See `AGENTS.md` for the full workflow including session initialization, issue title conventions (`[<branch>] description`), scoping rules, and completion checklist.

## Key Conventions

- Proto definitions live in `sdk/rust/api/` — Rust is the source of truth, Go is generated
- All services share a single port in local dev mode (`--port 23153`)
- Clippy must pass with `--deny warnings`
- The workspace uses `resolver = "2"`
