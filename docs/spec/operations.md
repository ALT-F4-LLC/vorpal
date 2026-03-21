---
project: "vorpal"
maturity: experimental
last_updated: "2026-03-20"
updated_by: "@staff-engineer"
scope: "CI/CD pipelines, deployment, release process, and infrastructure"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - security.md
---

# Operations

## Overview

Vorpal's operational infrastructure centers on GitHub Actions for CI/CD, a self-bootstrapping build system (Vorpal builds Vorpal), cross-platform release distribution, and Renovate for automated dependency management.

## CI/CD Pipelines

### Main Workflow (`.github/workflows/vorpal.yaml`)

Triggers: pull requests, pushes to `main`, tag pushes.

**Pipeline stages (sequential):**

1. **vendor** -- Restores/saves cargo vendor cache, runs `cargo check` in release mode. Runs on 4 runners: `macos-latest`, `macos-latest-large`, `ubuntu-latest`, `ubuntu-latest-arm64`.

2. **code-quality** -- Runs `cargo fmt --check` and `cargo clippy --deny warnings` in release mode. Single runner (`macos-latest`). Depends on `vendor`.

3. **build** -- Full release build + test + dist packaging. Verifies no non-system dynamic library dependencies (prevents accidental linking to homebrew/local libs). Uploads `vorpal-{arch}-{os}.tar.gz` artifacts. All 4 runners. Depends on `code-quality`.

4. **test** -- Integration tests using the built binary. Installs Vorpal via `setup-vorpal-action`, then builds multiple artifact types (`vorpal`, `vorpal-container-image`, `vorpal-job`, `vorpal-process`, `vorpal-shell`, `vorpal-user`). Verifies cross-SDK parity: Go and TypeScript SDKs must produce identical digests to Rust for each artifact. All 4 runners. Depends on `build`.

5. **release** (tag pushes only) -- Creates GitHub release with platform binaries, SLSA build provenance attestation. Depends on `test`.

6. **release-container-image** (tag pushes only) -- Builds container images via Vorpal self-hosting, pushes architecture-specific tags to Docker Hub (`altf4llc/vorpal`). Linux runners only. Depends on `test`.

7. **release-container-image-manifest** (tag pushes only) -- Creates multi-arch Docker manifest. Depends on `release-container-image`.

8. **release-sdk-rust** (non-nightly tags only) -- Publishes `vorpal-sdk` to crates.io. Checks version existence first to avoid duplicate publish errors. Depends on `test`.

9. **release-sdk-typescript** (non-nightly tags only) -- Publishes `@altf4llc/vorpal-sdk` to npm with provenance. Uses bun for build, node for publish. Checks version existence first. Depends on `test`.

### Nightly Workflow (`.github/workflows/vorpal-nightly.yaml`)

Triggers: daily at 08:00 UTC, manual dispatch.

- Deletes existing `nightly` release and tag
- Creates a new `nightly` tag pointing to latest `main` SHA
- Uses a GitHub App token (not PAT) for tag/release operations
- The main workflow's tag-trigger then handles the actual nightly build/release

### Concurrency

Both workflows use concurrency groups to cancel in-progress runs for the same PR or ref.

## Build System

### Development Build (without Vorpal)

The `makefile` provides standard development targets:

| Target | Command | Purpose |
|--------|---------|---------|
| `build` | `cargo build` | Debug build |
| `check` | `cargo check` | Type checking |
| `format` | `cargo fmt --all --check` | Format verification |
| `lint` | `cargo clippy -- --deny warnings` | Lint with warnings-as-errors |
| `test` | `cargo test` | Unit tests |
| `dist` | `tar -czf` | Package release binary |
| `vendor` | `cargo vendor --versioned-dirs` | Vendor dependencies |
| `generate` | `protoc` | Generate Go/TypeScript proto stubs |

All CI builds use vendored dependencies (offline mode) for reproducibility.

### Development Environment

- `script/dev.sh` -- Pre-bakes the development environment (used in CI)
- `script/dev/` -- Platform-specific setup scripts (debian, arch, rustup, protoc, etc.)
- Lima VM support for cross-platform testing (`lima.yaml` + `make lima-*` targets)
- Docker Compose with Keycloak for OIDC development

### Self-Hosted Build (with Vorpal)

Vorpal builds itself via a `vorpal-config` binary (`config/`). The `Vorpal.toml` at the project root configures this self-hosted build. Three config variants exist:
- `Vorpal.toml` (Rust SDK)
- `Vorpal.go.toml` (Go SDK)
- `Vorpal.ts.toml` (TypeScript SDK)

## Release Process

1. Push a semver tag (e.g., `v0.1.0-alpha.1`) to trigger release jobs
2. CI builds binaries for 4 platforms (aarch64-darwin, aarch64-linux, x86_64-darwin, x86_64-linux)
3. GitHub Release created with platform tarballs
4. Build provenance attestation generated
5. Container images pushed to Docker Hub with multi-arch manifest
6. Rust SDK published to crates.io (if version not already published)
7. TypeScript SDK published to npm with provenance (if version not already published)

Nightly releases happen automatically via the nightly workflow creating a `nightly` tag daily.

## Infrastructure

### Registry Backends

- **Local** -- File-system storage at `/var/lib/vorpal/`
- **S3** -- AWS S3 bucket (`altf4llc-vorpal-registry` in CI). Configured via `--registry-backend s3 --registry-backend-s3-bucket <name>`

### Identity Provider

Keycloak is used as the OIDC provider:
- Docker Compose for local development
- Terraform module (`terraform/module/keycloak/`) for production configuration
- Realm: `vorpal`

### Service Management

- macOS: LaunchAgent (user-level)
- Linux: systemd (system-level -- noted as inconsistent in TDD)
- The install script (`script/install.sh`) handles service setup

## Dependency Management

### Renovate

Configured via `.github/renovate.json` with granular automerge policies:

- GitHub Actions minor/patch: automerge
- Dev dependencies patch: automerge
- Production dependencies (Cargo, Go, npm, Docker): automerge with 3-day minimum release age for patch/minor (stable crates only)
- Terraform providers: never automerge
- Go indirect deps: never automerge
- Lock file maintenance: weekly, automerge

### Rust Toolchain

Pinned via `rust-toolchain.toml`:
- Channel: `1.93.1`
- Components: `clippy`, `rust-analyzer`, `rustfmt`
- Profile: `minimal`

## Observability

### Current State

- **Logging**: `tracing` + `tracing-subscriber` with configurable log levels (via `--level` flag). Debug/trace modes include file and line numbers. Logs written to stderr.
- **Health checks**: gRPC health service (`tonic-health`) with optional plaintext TCP endpoint for load balancer probes.
- **No metrics**: No Prometheus, StatsD, or similar metrics collection.
- **No distributed tracing**: No OpenTelemetry or Jaeger integration.

### Signal Handling

The server process handles SIGINT and SIGTERM for graceful shutdown, cleans up UDS socket files, and releases advisory file locks.

## Gaps and Areas for Improvement

- No monitoring or alerting infrastructure
- No metrics collection or dashboards
- No distributed tracing
- No structured audit logging (user context extraction exists but is unused)
- No rollback procedures documented
- No runbooks for operational issues
- System-level systemd vs user-level LaunchAgent inconsistency (noted in install TDD)
- No health check for individual service components (only aggregate gRPC health)
