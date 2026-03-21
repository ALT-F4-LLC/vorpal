---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Operational characteristics: CI/CD, deployment, service management, observability, and release processes"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - security.md
---

# Operations

## Overview

Vorpal is a build system with a client-server architecture. The server-side services (agent, registry, worker) run as a background daemon managed by OS-native service managers. The project uses GitHub Actions for CI/CD with a multi-platform build matrix, tag-driven releases, and automated nightly builds.

## CI/CD Pipeline

### Primary Workflow (`.github/workflows/vorpal.yaml`)

Triggered on all pull requests and pushes to `main` or any tag. Uses concurrency groups to cancel in-progress runs for the same PR or ref.

**Pipeline stages (sequential):**

1. **vendor** -- Restores Cargo vendor and target caches, runs `./script/dev.sh make .cargo vendor` followed by `make TARGET=release check` across a 4-runner matrix (macOS x86_64, macOS ARM64, Ubuntu x86_64, Ubuntu ARM64). Saves caches after completion.

2. **code-quality** -- Runs on `macos-latest` only. Executes `make format` (cargo fmt check) and `make TARGET=release lint` (clippy with `--deny warnings`).

3. **build** -- Runs on all 4 platforms. Builds release binaries, verifies no non-system dynamic library dependencies (checks for homebrew/local libs, liblzma, libzstd, liblz4, libbrotli), runs `cargo test`, and produces distribution tarballs (`vorpal-{arch}-{os}.tar.gz`). Uploads dist artifacts.

4. **test** -- Runs on all 4 platforms. Downloads built artifacts, sets up Vorpal via `ALT-F4-LLC/setup-vorpal-action@main` with S3 registry backend, then builds and validates Vorpal artifacts across all three SDKs (Rust, Go, TypeScript) to confirm cross-SDK deterministic output.

5. **release** -- Tag-only (`refs/tags/*`). Creates a GitHub release with pre-release flag. Uploads 4 platform tarballs and generates build provenance attestations via `actions/attest-build-provenance@v4`.

6. **release-container-image** -- Tag-only. Builds container images on Ubuntu x86_64 and ARM64 runners using Vorpal's own `vorpal-container-image` artifact. Pushes arch-specific images to `docker.io/altf4llc/vorpal:{tag}-{amd64|arm64}`.

7. **release-container-image-manifest** -- Tag-only, depends on `release-container-image`. Creates and pushes a multi-arch Docker manifest to `docker.io/altf4llc/vorpal:{tag}`.

8. **release-sdk-rust** -- Tag-only, non-nightly. Publishes `vorpal-sdk` crate to crates.io (skips if version already exists).

9. **release-sdk-typescript** -- Tag-only, non-nightly. Publishes `@altf4llc/vorpal-sdk` to npm with `--tag next` and `--provenance` (skips if version already exists).

### Nightly Workflow (`.github/workflows/vorpal-nightly.yaml`)

Runs daily at 08:00 UTC via cron schedule (also manually dispatchable). Deletes any existing `nightly` release and tag, then re-creates the `nightly` tag pointing at the current `main` HEAD SHA. This triggers the primary workflow's tag-based release pipeline, producing nightly builds.

Uses a GitHub App token (`ALTF4LLC_GITHUB_APP_ID` / `ALTF4LLC_GITHUB_APP_PRIVATE_KEY`) for tag and release management.

### Dependency Management (`.github/workflows/renovate.yaml` + `.github/renovate.json`)

Renovate bot manages dependency updates. The `renovate.yaml` workflow auto-approves Renovate PRs. The Renovate configuration:

- **Automerge policy:** GitHub Actions minor/patch, devDependencies patch (all ecosystems), devDependencies minor for stable (>=1.0) crates, production deps patch/minor (with 3-day minimum release age) for Cargo, Go modules, npm, and Docker.
- **No automerge:** Go indirect dependencies, Terraform providers.
- **Ignored:** Vorpal SDK updates in Go template directory.
- **Lock file maintenance:** Enabled weekly with automerge.

## Service Management

### Installation (`script/install.sh`)

A comprehensive installer script supporting:

- **Platforms:** macOS (x86_64, ARM64), Linux (x86_64, ARM64)
- **Modes:** Interactive (default), non-interactive (`VORPAL_NONINTERACTIVE=1` or `CI=true`), dry-run (`VORPAL_DRY_RUN=1`)
- **Version:** Configurable via `VORPAL_VERSION` (default: `nightly`)
- **Install path:** `~/.vorpal/bin/vorpal`
- **System data path:** `/var/lib/vorpal/` with subdirectories: `key/`, `sandbox/`, `store/artifact/{alias,archive,config,output}`, `log/`
- **Uninstall:** `--uninstall` flag with confirmation prompt (or `--yes` for non-interactive)
- **Upgrade-aware:** Detects existing installations, preserves keys, restarts services

### macOS: LaunchAgent

- Plist location: `~/Library/LaunchAgents/com.altf4llc.vorpal.plist`
- Runs `vorpal system services start`
- Configured with `RunAtLoad` and `KeepAlive` (auto-restart on failure)
- Logs to: `/var/lib/vorpal/log/services.log`
- Manual restart: `launchctl kickstart gui/<uid>/com.altf4llc.vorpal`

### Linux: systemd User Unit

- Unit location: `~/.config/systemd/user/vorpal.service`
- Runs `vorpal system services start`
- Configured with `Restart=on-failure`, `RestartSec=5`
- Wanted by `default.target`
- Logs via: `journalctl --user -u vorpal.service`
- Manual restart: `systemctl --user restart vorpal.service`
- Note: Requires `loginctl enable-linger` for services to persist after logout

### Service Architecture

The `vorpal system services start` command starts a gRPC server hosting configurable services:

- **Default services:** `agent,registry,worker`
- **Transport:** Unix domain socket at `/var/lib/vorpal/vorpal.sock` (default) or TCP with `--port` flag (default TCP port: 23151 when TLS enabled)
- **TLS:** Optional via `--tls` flag, requires keys in `/var/lib/vorpal/key/`
- **Health checks:** Optional plaintext gRPC health endpoint on `--health-check-port` (default: 23152), enabled with `--health-check` flag. Uses `tonic-health` for standard gRPC health checking protocol.
- **Registry backends:** `local` (default, filesystem-based) or `s3` (with `--registry-backend-s3-bucket`)
- **Auth:** Optional OIDC via `--issuer`, `--issuer-audience`, `--issuer-client-id`, `--issuer-client-secret`
- **Graceful shutdown:** Handles SIGINT and SIGTERM signals
- **Lock file:** File-based lock (`vorpal.lock` adjacent to the socket) prevents concurrent server instances

### Storage Management

- **Data root:** `/var/lib/vorpal/`
- **Store layout:** `store/artifact/{alias,archive,config,output}`
- **Sandbox:** `sandbox/` for isolated build environments
- **Keys:** `key/` for TLS certificates and credentials
- **Pruning:** `vorpal system prune` with granular flags: `--all`, `--artifact-aliases`, `--artifact-archives`, `--artifact-configs`, `--artifact-outputs`, `--sandboxes`

## Observability

### Logging

- Uses the `tracing` crate with `tracing-subscriber` for structured logging
- Log output goes to stderr
- Log level configurable via `--level` CLI flag (default: `INFO`)
- Debug/trace levels additionally include file name and line number
- No external log aggregation or shipping configured

### Health Checking

- Optional gRPC health check endpoint (standard `grpc.health.v1.Health` protocol via `tonic-health`)
- Plaintext listener on a separate port (default 23152) to support health probing even when main listener uses TLS
- No application-level health metrics beyond gRPC health status

### Gaps

- **No metrics collection:** No Prometheus, StatsD, or other metrics export
- **No distributed tracing:** No OpenTelemetry, Jaeger, or trace propagation
- **No alerting:** No alerting rules or integration with alerting systems
- **No dashboards:** No Grafana, Datadog, or other dashboard configurations
- **No log aggregation:** Logs are local only (stderr or OS service log files)
- **No structured error codes:** Errors use ad-hoc string messages
- **No runbooks:** No operational runbooks exist in the repository

## Development Environment

### Setup Scripts

- **`script/dev.sh`** -- Bootstrap script that installs development dependencies (rustup, protoc, and on non-CI: xz, amber, terraform). Detects Linux distribution (Debian/Ubuntu, Arch) for system package installation. Sets up `.env/bin` directory for tool isolation.
- **`script/dev/debian.sh`** -- Installs Debian/Ubuntu system packages: bubblewrap, build-essential, ca-certificates, curl, jq, rsync, unzip, Docker.
- **`script/dev/arch.sh`** -- Arch Linux equivalent.

### Build System (makefile)

Key targets:

| Target | Description |
|--------|-------------|
| `build` | `cargo build` (default goal) |
| `check` | `cargo check` |
| `format` | `cargo fmt --all --check` |
| `lint` | `cargo clippy --deny warnings` |
| `test` | `cargo test` |
| `dist` | Creates release tarball |
| `vendor` | Vendors Cargo dependencies |
| `generate` | Regenerates protobuf code for Go and TypeScript SDKs |
| `vorpal` | Runs Vorpal build via cargo |
| `vorpal-start` | Starts Vorpal services via cargo |

Supports `TARGET=release` for release builds and `VERBOSE` for non-silent output.

### Lima Virtual Machine

- Lima VM configuration (`lima.yaml`) for Linux development on macOS
- Uses Debian 12 (Bookworm) cloud images for both x86_64 and aarch64
- Makefile targets: `lima` (create/start VM), `lima-sync` (rsync project), `lima-vorpal` (run builds in VM), `lima-vorpal-start` (start services in VM)
- Configurable CPU, disk, and memory via make variables

### Linux Slimming Script (`script/linux-vorpal-slim.sh`)

Reduces Vorpal Linux rootfs installations from ~2.9GB to ~600-700MB by removing development tools, documentation, and unnecessary localization files. Supports dry-run mode (default), backup creation, and aggressive mode.

## Release Process

### Versioned Releases

1. A git tag is pushed (e.g., `v0.x.x`)
2. The primary CI workflow detects the tag push
3. Builds and tests run across all 4 platforms
4. On success: GitHub release created (pre-release), binaries uploaded, provenance attested
5. Container images built, tagged, and pushed to Docker Hub with multi-arch manifest
6. SDK packages published to crates.io and npm (if version is new and tag is not `nightly`)

### Nightly Releases

1. Daily cron at 08:00 UTC (or manual dispatch)
2. Existing `nightly` release and tag are deleted
3. New `nightly` tag created at current `main` HEAD
4. Triggers standard release pipeline (all steps except SDK publishing)

### Rollback

- **No formal rollback procedure exists.** All GitHub releases are marked as pre-release.
- Previous release artifacts remain available on GitHub Releases.
- Docker images are tagged per-version; previous versions remain in the registry.
- SDK rollback would require publishing a new version or yanking from crates.io/npm.

## Container Image

- Built using Vorpal's own build system (`vorpal build "vorpal-container-image"`)
- Published to: `docker.io/altf4llc/vorpal:{tag}` with multi-arch manifest (amd64 + arm64)
- Linux-only (built on Ubuntu runners)

## Secrets and Configuration

### CI Secrets

| Secret | Purpose |
|--------|---------|
| `AWS_ACCESS_KEY_ID` | S3 registry backend for CI testing |
| `AWS_SECRET_ACCESS_KEY` | S3 registry backend for CI testing |
| `CARGO_REGISTRY_TOKEN` | Publishing to crates.io |
| `DOCKERHUB_TOKEN` | Docker Hub image push |
| `DOCKERHUB_USERNAME` | Docker Hub authentication |
| `ALTF4LLC_GITHUB_APP_ID` | Nightly release automation |
| `ALTF4LLC_GITHUB_APP_PRIVATE_KEY` | Nightly release automation |

### CI Variables

| Variable | Purpose |
|----------|---------|
| `AWS_DEFAULT_REGION` | AWS region for S3 registry |

### Runtime Configuration

- Socket path: `VORPAL_SOCKET_PATH` environment variable (overrides default `/var/lib/vorpal/vorpal.sock`)
- All service configuration via CLI flags (no config file for the server)
- Client configuration via `Vorpal.toml` (project-level) and `~/.vorpal/settings.json` (user-level)
