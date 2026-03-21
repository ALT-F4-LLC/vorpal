---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "CI/CD pipelines, release processes, service management, infrastructure, and operational procedures"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - security.md
---

# Operations

## Overview

This document describes the operational aspects of the Vorpal project as they exist today: CI/CD pipelines, release processes, service lifecycle management, infrastructure provisioning, dependency management, and observability. Vorpal is a build system implemented in Rust with SDKs in Go and TypeScript, distributed as native binaries and Docker images across four platform targets.

## CI/CD Pipelines

### Primary Pipeline (`vorpal.yaml`)

Triggered on every push to `main` and on all pull requests. Uses GitHub Actions with a concurrency group that cancels in-progress runs for the same PR or ref.

**Pipeline stages (sequential):**

1. **Vendor** -- Runs on a 4-runner matrix (macos-latest, macos-latest-large, ubuntu-latest, ubuntu-latest-arm64). Checks out code, restores `target/` and `vendor/` caches keyed by `{arch}-{os}-{Cargo.lock hash}`, runs `./script/dev.sh` to bootstrap the development environment, then executes `make .cargo vendor` and `make TARGET=release check`. Saves caches after completion.

2. **Code Quality** -- Depends on vendor. Runs on macos-latest only. Executes `make format` (cargo fmt --all --check) and `make TARGET=release lint` (cargo clippy -- --deny warnings).

3. **Build** -- Depends on code-quality. Same 4-runner matrix. Builds release binaries (`make TARGET=release build`), verifies no non-system dynamic library dependencies are linked (checks for homebrew/local libs, liblzma, libzstd, liblz4, libbrotli using `otool -L` on macOS and `ldd` on Linux), runs unit tests (`make TARGET=release test`), creates distribution tarballs (`make TARGET=release dist`), and uploads artifacts as `vorpal-dist-{arch}-{os}`.

4. **Test** -- Depends on build. Same 4-runner matrix. Downloads built artifacts, sets up Vorpal using `ALT-F4-LLC/setup-vorpal-action@main` with S3 registry backend (`altf4llc-vorpal-registry` bucket), then exercises the build system end-to-end by building multiple artifact types (`vorpal`, `vorpal-container-image`, `vorpal-job`, `vorpal-process`, `vorpal-shell`, `vorpal-user`) using the Rust config, then verifying that Go SDK and TypeScript SDK produce identical artifact hashes for each. Container image builds are Linux-only.

5. **Release** -- Tag-triggered only (`refs/tags/*`). Downloads all platform artifacts and creates a GitHub release (marked as prerelease) with four tarballs: `vorpal-{aarch64,x86_64}-{darwin,linux}.tar.gz`. Generates build provenance attestations via `actions/attest-build-provenance@v4`.

6. **Release Container Image** -- Tag-triggered. Runs on ubuntu-latest and ubuntu-latest-arm64. Builds container images using Vorpal itself, loads into Docker, tags as `docker.io/altf4llc/vorpal:{tag}-{amd64|arm64}`, pushes to Docker Hub.

7. **Release Container Image Manifest** -- Tag-triggered. Depends on release-container-image. Creates and pushes a multi-arch Docker manifest combining the amd64 and arm64 images under `docker.io/altf4llc/vorpal:{tag}`.

8. **Release SDK Rust** -- Tag-triggered, non-nightly tags only. Checks if `vorpal-sdk` version already exists on crates.io; publishes only if it does not.

9. **Release SDK TypeScript** -- Tag-triggered, non-nightly tags only. Checks if `@altf4llc/vorpal-sdk` version already exists on npm; publishes with `--provenance --tag next` only if it does not.

### Nightly Pipeline (`vorpal-nightly.yaml`)

Scheduled daily at 08:00 UTC, also manually dispatchable. Uses a GitHub App token to:
1. Delete the existing `nightly` release and tag (if present).
2. Read the SHA of `main`.
3. Create a new `nightly` tag pointing at that SHA.

This triggers the primary pipeline's release jobs (since `nightly` matches `refs/tags/*`), producing nightly builds. The nightly tag explicitly excludes SDK publishing (gated by `!contains(github.ref, 'nightly')`).

### Renovate Pipeline (`renovate.yaml`)

Triggered on `pull_request_target` events. Auto-approves PRs from `renovate[bot]` using `gh pr review --approve`.

## Dependency Management

Managed by Renovate with a detailed configuration in `.github/renovate.json`:

- **Lock file maintenance**: Weekly, automerged.
- **GitHub Actions**: Minor and patch updates automerged.
- **Dev dependencies** (all ecosystems): Patch updates automerged; minor updates automerged for stable (>= 1.0) packages only.
- **Production dependencies** (Cargo, Go modules, npm, Docker): Patch and stable-minor updates automerged with a 3-day minimum release age.
- **Excluded from automerge**: Go indirect dependencies, Terraform providers.
- **Ignored entirely**: Vorpal SDK Go module updates in the Go template directory (`cli/src/command/template/go/**`).

Renovate PRs are auto-approved via the `renovate.yaml` workflow and platform-automerged when all CI checks pass.

## Service Lifecycle Management

### Service Architecture

Vorpal services run as a single binary (`vorpal system services start`) hosting multiple gRPC services in one process:

- **Agent** -- Local build agent for executing artifact builds.
- **Archive** -- Content-addressable storage for build artifacts.
- **Artifact** -- Metadata and alias registry for artifacts.
- **Worker** -- Remote build worker coordination.

Services are selectable via the `--services` flag (comma-separated). Default listening mode is Unix domain socket (`/tmp/vorpal-{dir}.sock`); TCP mode activated with `--port` (default 23151 when TLS enabled).

### Health Checking

gRPC health checking (tonic-health) is available via `--health-check` flag. When enabled, a separate plaintext TCP health server runs on `--health-check-port` (must differ from the main service port). Per-service health status is reported (agent, archive, artifact, worker) as they become ready.

### Process Management

- **Lock file**: Advisory file lock (`fs4`) prevents multiple instances on the same socket path (TOCTOU-safe).
- **Stale socket detection**: On startup, attempts to connect to an existing socket; removes it only if connection is refused (stale), bails if it connects (live instance) or is permission-denied.
- **Signal handling**: Graceful shutdown on SIGINT and SIGTERM via `tokio::signal`. Socket file cleaned up on exit; lock file left on disk (released via drop).

### Service Installation

The installer (`script/install.sh`) sets up platform-appropriate service management:

**macOS (LaunchAgent):**
- Installs `com.altf4llc.vorpal` plist to `~/Library/LaunchAgents/`.
- Configured with `RunAtLoad: true` and `KeepAlive: true` for automatic restart.
- Logs to `/var/lib/vorpal/log/services.log` (both stdout and stderr).
- Managed via `launchctl bootstrap/bootout gui/{uid}`.

**Linux (systemd user unit):**
- Installs `vorpal.service` to `~/.config/systemd/user/`.
- Configured as `Type=simple` with `Restart=on-failure`, `RestartSec=5`.
- `WantedBy=default.target` for auto-start on login.
- Managed via `systemctl --user {start|stop|restart|enable} vorpal.service`.
- Logs via `journalctl --user -u vorpal.service`.

### Registry Backends

Two storage backends for archive and artifact data:
- **Local**: Filesystem-based storage (default).
- **S3**: AWS S3-backed storage, requires `--registry-backend-s3-bucket`. Supports `--registry-backend-s3-force-path-style` for S3-compatible endpoints.

CI uses S3 backend (`altf4llc-vorpal-registry` bucket) with AWS credentials from GitHub secrets.

## Infrastructure

### Terraform (Development Environment)

Located in `terraform/`, provisions a development environment on AWS:

**Networking:**
- Single-AZ VPC (`10.42.0.0/16`) with one public subnet (`10.42.0.0/24`).
- No NAT gateway (public-only).
- Security group allows all ingress from a configurable CIDR (`ssh_ingress_cidr`, defaults to `0.0.0.0/0`) and all egress.

**Compute instances:**
- `vorpal-dev-registry`: t4g.large (ARM64 Ubuntu 24.04), 100GB EBS. Registry server.
- `vorpal-dev-worker-aarch64-linux`: t4g.large (ARM64 Ubuntu 24.04), 100GB EBS.
- `vorpal-dev-worker-x8664-linux`: t3a.large (x86_64 Ubuntu 24.04), 100GB EBS.
- `vorpal-dev-worker-aarch64-darwin`: mac2.metal (Apple Silicon), optional via `create_mac_instances` variable. Requires a dedicated host.
- `vorpal-dev-worker-x8664-darwin`: mac1.metal (Intel Mac), optional. Requires a dedicated host.

**Key management:**
- Auto-generated SSH key pair stored in AWS SSM Parameter Store (`/vorpal-dev/private-key`, SecureString).

### Keycloak (Identity Provider)

- **Local development**: Docker Compose runs Keycloak 26.5.5 in dev mode on `localhost:8080`.
- **Terraform configuration**: Provisions a `vorpal` realm with OpenID Connect clients for `cli` (public, device auth grant), `archive` (confidential), `artifact` (confidential), and `worker` (confidential, with service account). RBAC roles defined per client (e.g., `archive:push`, `artifact:store`, `worker:build-artifact`). Client scopes with audience and role mappers link services together. A default admin user is provisioned.

### Lima (Local Linux Development)

Lima configuration (`lima.yaml`) provides Linux VMs for macOS developers:
- Debian 12 (Bookworm) cloud images for amd64 and aarch64.
- Configurable via makefile targets: `make lima` (create/start), `make lima-sync`, `make lima-vorpal`.
- Default: 8 CPUs, 8GB RAM, 100GB disk.

## Development Environment

### Bootstrap Script (`script/dev.sh`)

Entry point for all development and CI commands. Ensures required tooling is installed:
- **All environments**: rustup, protoc (for gRPC codegen).
- **Non-CI only**: xz, amber (secrets management), terraform.
- **Linux**: Platform-specific package installation (debian.sh, arch.sh).

Usage pattern: `./script/dev.sh <command>` wraps any command with the correct `PATH` and environment.

### Makefile Targets

Development without Vorpal:
- `make build` / `make check` / `make test` / `make lint` / `make format` -- Standard Cargo commands with optional `TARGET=release`.
- `make vendor` -- Vendor Cargo dependencies.
- `make dist` -- Create distribution tarballs.
- `make generate` -- Regenerate Go and TypeScript SDK code from protobuf definitions.
- `make clean` -- Remove build artifacts, cargo config, vendor, and dist directories.

Development with Vorpal (self-hosting):
- `make vorpal` -- Build using Vorpal itself.
- `make vorpal-start` -- Start Vorpal services locally.

### Installer (`script/install.sh`)

End-user installer, curl-pipeable:
```
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash
```

Features:
- Platform detection (macOS/Linux, x86_64/aarch64).
- Version resolution from GitHub releases (default: nightly).
- Upgrade detection with existing version comparison.
- Dry-run mode (`VORPAL_DRY_RUN=1`).
- Non-interactive mode (`VORPAL_NONINTERACTIVE=1` or `CI=true`).
- Uninstall support (`--uninstall`).
- Configurable: skip service installation (`--no-service`), skip PATH configuration (`--no-path`).
- Creates `/var/lib/vorpal` for artifact storage and service logs (requires sudo).

## Release Process

### Versioned Releases

1. Tag a commit on `main` (e.g., `v0.x.y`).
2. The primary pipeline runs all stages through test.
3. Release jobs produce:
   - GitHub release with 4 platform tarballs and build provenance attestations.
   - Multi-arch Docker image on Docker Hub (`altf4llc/vorpal:{tag}`).
   - Rust SDK on crates.io (if version is new).
   - TypeScript SDK on npm with `next` tag (if version is new).
4. All releases are marked as prerelease.

### Nightly Releases

Automated daily at 08:00 UTC:
1. Nightly workflow deletes old `nightly` tag/release.
2. Creates new `nightly` tag at current `main` HEAD.
3. Triggers the full release pipeline (binaries + Docker images, but NOT SDK publishes).

### Rollback

No formal rollback procedure exists. Recovery options:
- **Binaries**: Previous releases remain on GitHub; users can pin `VORPAL_VERSION` in the installer.
- **Docker images**: Previous tags remain on Docker Hub.
- **SDKs**: Published crate/npm versions are immutable; must publish a new patch.

## Observability

### Current State

**Logging:**
- Structured logging via `tracing` / `tracing-subscriber` crates.
- Log levels: debug, info, warn, error. Used throughout the CLI and service code.
- Service logs routed to `/var/lib/vorpal/log/services.log` when running as a system service.
- Linux: Also accessible via `journalctl --user -u vorpal.service`.

### Gaps

- **No metrics collection**: No Prometheus, StatsD, or equivalent metrics instrumentation.
- **No distributed tracing**: No OpenTelemetry or trace propagation between services.
- **No alerting**: No automated alerting on service health or build failures.
- **No dashboards**: No Grafana or equivalent monitoring dashboards.
- **No centralized log aggregation**: Logs are local to each host; no shipping to a central system.
- **No structured error reporting**: No Sentry or equivalent error tracking integration.
- **No SLIs/SLOs defined**: No formal service level indicators or objectives.

The health check endpoint (gRPC health protocol) is the only runtime observability mechanism. There is no readiness/liveness distinction beyond the binary health check.

## Operational Runbooks

No formal runbooks exist. Troubleshooting guidance is embedded in installer error messages:
- LaunchAgent failures: Check `/var/lib/vorpal/log/services.log`, common causes (port conflict, permissions).
- systemd failures: `journalctl --user -u vorpal.service --no-pager -n 20`.
- Manual restart commands provided in error output.

## Platform Support Matrix

| Platform | Architecture | CI Build | CI Test | Release Binary | Docker Image |
|----------|-------------|----------|---------|---------------|-------------|
| macOS    | aarch64     | Yes      | Yes     | Yes           | No          |
| macOS    | x86_64      | Yes      | Yes     | Yes           | No          |
| Linux    | aarch64     | Yes      | Yes     | Yes           | Yes         |
| Linux    | x86_64      | Yes      | Yes     | Yes           | Yes         |

## Secrets and Credentials

Managed via GitHub Actions secrets (not committed to the repository):
- `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` -- S3 registry backend access.
- `DOCKERHUB_TOKEN` / `DOCKERHUB_USERNAME` -- Docker Hub image publishing.
- `CARGO_REGISTRY_TOKEN` -- crates.io publishing.
- `ALTF4LLC_GITHUB_APP_ID` / `ALTF4LLC_GITHUB_APP_PRIVATE_KEY` -- GitHub App for nightly tag management.

GitHub Actions variables:
- `AWS_DEFAULT_REGION` -- AWS region for S3 operations.

npm publishing uses OIDC-based provenance (`id-token: write` permission) rather than a static npm token.
