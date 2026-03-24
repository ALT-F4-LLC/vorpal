---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "CI/CD pipelines, release process, deployment, service management, and operational tooling"
owner: "@staff-engineer"
dependencies:
  - security.md
---

# Operations Specification

This document describes the operational infrastructure, CI/CD pipelines, release processes,
service management, and maintenance tooling that exist in the vorpal project as of the
`last_updated` date.

---

## 1. CI/CD Pipelines

All CI/CD is implemented as GitHub Actions workflows in `.github/workflows/`.

### 1.1 Primary Pipeline: `vorpal.yaml`

Triggers on all pull requests and pushes to `main` or any tag. Uses concurrency groups to
cancel in-progress runs for the same PR or ref.

**Job dependency chain:** `vendor` -> `code-quality` -> `build` -> `test` -> `release*`

#### 1.1.1 `vendor` Job

- **Runners:** 4-platform matrix (`macos-latest`, `macos-latest-large`, `ubuntu-latest`,
  `ubuntu-latest-arm64`)
- **Purpose:** Restore/save Cargo vendor and target caches, run `make check` in release mode
- **Caching:** Uses `actions/cache` keyed on `{arch}-{os}-{Cargo.lock hash}` for both
  `target/` and `vendor/` directories
- **Entry point:** `./script/dev.sh make .cargo vendor` followed by
  `./script/dev.sh make TARGET=release check`

#### 1.1.2 `code-quality` Job

- **Runner:** `macos-latest` only (single platform)
- **Steps:** Format check (`make format`) and lint (`make TARGET=release lint`)

#### 1.1.3 `build` Job

- **Runners:** Same 4-platform matrix
- **Steps:**
  - Release build (`make TARGET=release build`)
  - **Dynamic dependency verification:** Ensures binaries have no non-system dynamic library
    dependencies (checks for homebrew, liblzma, libzstd, liblz4, libbrotli). Uses `otool -L`
    on macOS and `ldd` on Linux.
  - Release tests (`make TARGET=release test`)
  - Distribution packaging (`make TARGET=release dist`)
- **Artifacts:** Uploads `vorpal-dist-{arch}-{os}` containing `.tar.gz` archives

#### 1.1.4 `test` Job (Integration)

- **Runners:** Same 4-platform matrix
- **Purpose:** End-to-end build verification using `vorpal build` against multiple SDK
  configurations (Rust, Go, TypeScript)
- **Infrastructure:** Uses `ALT-F4-LLC/setup-vorpal-action@main` with S3 registry backend
  (`altf4llc-vorpal-registry` bucket)
- **AWS credentials:** Provided via repository secrets (`AWS_ACCESS_KEY_ID`,
  `AWS_SECRET_ACCESS_KEY`) and variables (`AWS_DEFAULT_REGION`)
- **Verification:** Builds each artifact with Rust config (`Vorpal.toml`), Go config
  (`Vorpal.go.toml`), and TypeScript config (`Vorpal.ts.toml`), then asserts cross-SDK
  digest consistency (Go output must match Rust, TypeScript output must match Rust)
- **Container image builds:** Only on `ubuntu-*` runners (Linux-only)
- **Lock file:** Uploads `Vorpal.lock` as artifact per platform

### 1.2 Release Jobs (Tag-Triggered)

All release jobs are gated on: `github.event_name == 'push' && contains(github.ref, 'refs/tags/')`

#### 1.2.1 `release` Job

- Collects all `vorpal-dist-*` artifacts
- Creates a GitHub Release via `softprops/action-gh-release@v2` with all 4 platform tarballs
  - `vorpal-aarch64-darwin.tar.gz`
  - `vorpal-aarch64-linux.tar.gz`
  - `vorpal-x86_64-darwin.tar.gz`
  - `vorpal-x86_64-linux.tar.gz`
- **All releases are marked as prerelease** (`prerelease: true`)
- **Build provenance attestation** via `actions/attest-build-provenance@v4` for all 4 binaries
- **Permissions:** `attestations: write`, `contents: write`, `id-token: write`, `packages: write`

#### 1.2.2 `release-container-image` Job

- **Runners:** `ubuntu-latest` and `ubuntu-latest-arm64` (Linux multi-arch)
- Builds container image using vorpal itself (`vorpal build --path "vorpal-container-image"`)
- Loads image via `docker image load`
- Pushes to DockerHub as `docker.io/altf4llc/vorpal:{tag}-{amd64|arm64}`
- **Credentials:** `DOCKERHUB_TOKEN` and `DOCKERHUB_USERNAME` secrets

#### 1.2.3 `release-container-image-manifest` Job

- Runs after `release-container-image` completes
- Creates and pushes a multi-arch Docker manifest combining `amd64` and `arm64` images
- Final image: `docker.io/altf4llc/vorpal:{tag}`

#### 1.2.4 `release-sdk-rust` Job

- **Gated additionally on:** tag must NOT contain `nightly`
- Checks if version already exists on crates.io before publishing
- Publishes `vorpal-sdk` crate via `cargo publish`
- **Credential:** `CARGO_REGISTRY_TOKEN` secret

#### 1.2.5 `release-sdk-typescript` Job

- **Gated additionally on:** tag must NOT contain `nightly`
- Uses Bun for build, Node.js 24 for publish
- Checks if version already exists on npm before publishing
- Publishes `@altf4llc/vorpal-sdk` with `--provenance --tag next`
- **Permissions:** `id-token: write` for npm OIDC provenance

### 1.3 Nightly Pipeline: `vorpal-nightly.yaml`

- **Schedule:** Daily at 08:00 UTC via cron (`0 8 * * *`), plus manual `workflow_dispatch`
- **Process:**
  1. Generates a GitHub App token (`ALTF4LLC_GITHUB_APP_ID` / `ALTF4LLC_GITHUB_APP_PRIVATE_KEY`)
  2. Deletes existing `nightly` release and tag (if present)
  3. Gets the SHA of `main` branch HEAD
  4. Creates a new `nightly` tag pointing at that SHA
- This triggers the main `vorpal.yaml` pipeline (which responds to tag pushes), which in turn
  creates a nightly release. SDK publishes are skipped for nightly tags.

### 1.4 Renovate: `renovate.yaml`

- **Trigger:** `pull_request_target` events from `renovate[bot]`
- **Action:** Auto-approves Renovate PRs via `gh pr review --approve`
- **Renovate config** (`.github/renovate.json`):
  - Base config with semantic commit prefix `chore`
  - Weekly lock file maintenance with automerge
  - Automerge rules by ecosystem (GitHub Actions, Cargo, Go modules, npm, Docker) with
    tiered policies: minor+patch for dev deps, patch-only for production deps with 3-day
    minimum release age
  - Explicit exclusions: Go indirect deps, Terraform providers, and Vorpal SDK in Go template
    directories

---

## 2. Development Environment

### 2.1 Entry Point: `script/dev.sh`

All CI and local development commands flow through `script/dev.sh`, which:

1. Sets up `PATH` to include `.env/bin` and `~/.cargo/bin`
2. Detects Linux distribution and installs system dependencies:
   - Debian/Ubuntu: `script/dev/debian.sh`
   - Arch: `script/dev/arch.sh`
   - Other: prints manual install instructions (bubblewrap, ca-certificates, curl, unzip, docker)
3. Installs toolchain dependencies via individual scripts in `script/dev/`:
   - **CI mode** (`CI=true`): `rustup`, `protoc`
   - **Local mode:** adds `xz`, `amber`, `lima`, `terraform`
4. Passes remaining arguments to execute (e.g., `./script/dev.sh make TARGET=release build`)

### 2.2 Build System

The project uses `make` as its build task runner (invoked via `./script/dev.sh make ...`).
No `Makefile` exists at the repository root -- the `make` target appears to be a custom command
or alias set up by the dev environment tooling. Build targets observed in CI:

| Target | Purpose |
|--------|---------|
| `.cargo vendor` | Vendor Cargo dependencies |
| `check` | Run cargo check |
| `format` | Run formatter (check mode) |
| `lint` | Run clippy/linter |
| `build` | Compile binaries |
| `test` | Run unit tests |
| `dist` | Package distribution archives |

The `TARGET=release` environment variable switches between debug and release profiles.

### 2.3 Lima Integration

`script/lima.sh` provides a Lima VM workflow for Linux development on macOS:

- `deps`: Installs Debian dependencies
- `sync`: Rsyncs the project (excluding `.env`, `.git`, `dist`, `target`) to `~/vorpal` in
  the VM and runs `./script/dev.sh make`
- `install`: Syncs + creates the Vorpal system directory structure and generates keys

### 2.4 System Prerequisites

`script/version_check.sh` validates minimum versions of build tools (Coreutils 8.1, Bash 3.2,
GCC 5.2, Make 4.0, Python 3.4, Linux kernel 4.19, etc.). This appears to be an LFS-derived
version check script.

---

## 3. Installation and Distribution

### 3.1 Install Script: `script/install.sh`

A comprehensive user-facing installer invocable via:
```
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash
```

**Features:**
- Platform detection (macOS/Linux, x86_64/aarch64)
- Version selection via `VORPAL_VERSION` env var (default: `nightly`)
- Downloads pre-built binaries from GitHub Releases
- Upgrade detection (preserves existing installation data)
- Interactive/non-interactive modes (`VORPAL_NONINTERACTIVE=1`, `CI=true`, `-y` flag)
- Dry-run mode (`VORPAL_DRY_RUN=1`)
- Uninstall support (`--uninstall` flag)
- Color/unicode detection with `NO_COLOR` support
- Configurable: `VORPAL_NO_SERVICE=1` (skip service install), `VORPAL_NO_PATH=1` (skip PATH config)

**Install location:** `$HOME/.vorpal`

### 3.2 Distribution Artifacts

| Artifact | Format | Platforms |
|----------|--------|-----------|
| CLI binary | `.tar.gz` | aarch64-darwin, aarch64-linux, x86_64-darwin, x86_64-linux |
| Container image | Docker manifest | linux/amd64, linux/arm64 |
| Rust SDK | crates.io crate | Platform-independent |
| TypeScript SDK | npm package | Platform-independent |

### 3.3 Container Image

Published to `docker.io/altf4llc/vorpal:{tag}` as a multi-arch manifest (amd64 + arm64).
The image is built by vorpal itself (`vorpal build "vorpal-container-image"`), making the
project self-hosting for container image production.

---

## 4. Service Architecture and Runtime

### 4.1 System Directory Layout

All runtime state lives under `/var/lib/vorpal/`:

```
/var/lib/vorpal/
  vorpal.sock          # Unix domain socket (default IPC)
  vorpal.lock          # Advisory lock file (prevents duplicate instances)
  key/
    ca.key.pem         # CA private key
    ca.pem             # CA certificate (via get_key_ca_path in SDK)
    service.key.pem    # Service private key
    service.pem        # Service certificate (signed by CA)
    service.public.pem # Service public key
    service.secret     # Service secret (UUID v7)
    credentials.json   # OAuth2 credentials store
  sandbox/
    {uuid-v7}/         # Ephemeral build sandboxes
  store/
    artifact/
      alias/           # Artifact name -> digest mappings
        {namespace}/
          {system}/
            {name}/{tag}
      archive/         # Compressed artifact archives
        {namespace}/
          {digest}.tar.zst
      config/          # Artifact configuration metadata
        {namespace}/
          {digest}.json
      output/          # Built artifact outputs
        {namespace}/
          {digest}/
          {digest}.lock.json
```

The socket path can be overridden via `VORPAL_SOCKET_PATH` environment variable.

### 4.2 Service Management (`vorpal system services start`)

The `system services start` command launches a gRPC server hosting configurable services:

| Service | Default | Description |
|---------|---------|-------------|
| `agent` | Enabled | Build agent service |
| `registry` | Enabled | Archive + artifact registry |
| `worker` | Enabled | Build worker service |

**Listener modes:**
- **Unix domain socket** (default): Binds to `/var/lib/vorpal/vorpal.sock` with `0o660` permissions
- **TCP**: Enabled via `--port` flag or implicitly when `--tls` is set (default port: 23151)

**TLS support:**
- Enabled via `--tls` flag
- Requires keys in `/var/lib/vorpal/key/` (generated by `vorpal system keys generate`)
- Uses self-signed CA -> service certificate chain

**Health checking:**
- Optional plaintext health check endpoint via `--health-check` flag
- Runs on separate TCP port (default: 23152, configurable via `--health-check-port`)
- Uses `tonic-health` (gRPC health checking protocol)
- Reports per-service health status

**Registry backends:**
- `local` (default): Filesystem-backed storage
- `s3`: AWS S3-backed storage (requires `--registry-backend-s3-bucket`)
  - Optional `--registry-backend-s3-force-path-style` for S3-compatible endpoints

**OIDC authentication:**
- Optional via `--issuer` flag
- Validates JWT tokens using OIDC discovery
- Applied as interceptors on archive, artifact, and worker services
- Supports configurable audience via `--issuer-audience`

**Instance management:**
- Advisory file lock (`vorpal.lock`) prevents duplicate instances (TOCTOU-safe)
- Stale socket detection: attempts connection before removing leftover socket files
- Graceful shutdown on SIGINT and SIGTERM with socket cleanup

### 4.3 Key Generation (`vorpal system keys generate`)

Generates a full PKI chain:
1. CA keypair (RSA, PKCS RSA SHA256) if not already present
2. Self-signed CA certificate (Country: US, Org: Vorpal, CA: unconstrained)
3. Service keypair
4. Service certificate (signed by CA, SAN: localhost, server auth EKU)
5. Service public key (extracted from keypair)
6. Service secret (UUID v7)

All key operations are idempotent -- existing keys are never overwritten.

### 4.4 Store Pruning (`vorpal system prune`)

Provides selective cleanup of local store data:

| Flag | Clears |
|------|--------|
| `--artifact-aliases` | `store/artifact/alias/` |
| `--artifact-archives` | `store/artifact/archive/` |
| `--artifact-configs` | `store/artifact/config/` |
| `--artifact-outputs` | `store/artifact/output/` |
| `--sandboxes` | `sandbox/` |
| `--all` | All of the above |

Reports freed disk space per category and total.

---

## 5. Observability

### 5.1 Logging

- Uses `tracing` + `tracing-subscriber` with structured logging
- Output goes to **stderr** (stdout is reserved for artifact output)
- Default level: `INFO`, configurable via `--level` global CLI flag
- `DEBUG` and `TRACE` levels additionally include file name and line number
- No timestamp in output (`.without_time()`)

### 5.2 Health Checks

- gRPC health protocol via `tonic-health`
- Per-service health status (agent, archive, artifact, worker)
- Available on a separate plaintext TCP port when enabled

### 5.3 Gaps

- **No metrics/telemetry:** No Prometheus, OpenTelemetry, or StatsD integration
- **No distributed tracing:** No trace propagation headers or span export
- **No alerting:** No built-in alert rules or integration with monitoring platforms
- **No dashboards:** No Grafana dashboards or similar operational visibility
- **No structured error reporting:** Errors use anyhow/tracing but no centralized error
  aggregation (e.g., Sentry)
- **No audit logging:** No record of who built what, when, or authentication events
- **No log aggregation:** Logs are local to the process stderr

---

## 6. Authentication and Authorization Testing

`script/test/keycloak.sh` provides a comprehensive OAuth2/OIDC integration test script:

- Tests device authorization flow against a local Keycloak instance
- Validates token exchange (worker -> artifact, worker -> archive)
- Tests token introspection
- Requires `docker-compose.yaml` Keycloak service running locally

`docker-compose.yaml` provides a local Keycloak instance (`quay.io/keycloak/keycloak:26.5.5`)
on `127.0.0.1:8080` with `start-dev` mode and default admin credentials.

---

## 7. Dependency Management

### 7.1 Renovate Bot

Automated dependency updates via Renovate with tiered automerge policies:

| Category | Automerge Policy |
|----------|-----------------|
| GitHub Actions (minor + patch) | Immediate automerge |
| Dev dependencies (patch) | Immediate automerge |
| Dev dependencies (minor, >= 1.0) | Immediate automerge |
| Production deps (patch) | Automerge after 3-day delay |
| Production deps (minor, >= 1.0) | Automerge after 3-day delay |
| Go indirect deps | No automerge |
| Terraform providers | No automerge |
| Pre-1.0 production deps (major/minor) | No automerge |

The Renovate workflow auto-approves its own PRs. Combined with `platformAutomerge: true`,
qualifying PRs merge automatically once CI passes.

---

## 8. Rollback and Recovery

### 8.1 What Exists

- **GitHub Releases:** All releases are tagged and archived; reverting means re-deploying a
  previous tag's artifacts
- **Install script:** Supports version pinning via `VORPAL_VERSION`, enabling rollback to a
  specific release
- **Idempotent key generation:** Keys survive reinstallation
- **Store pruning:** Selective cleanup without destroying keys

### 8.2 Gaps

- **No rollback automation:** No CLI command or script to revert to a previous version
- **No blue/green or canary deployment:** Single-binary replacement model
- **No database migrations:** Store is filesystem-based with no versioned schema
- **No backup/restore:** No tooling to backup or restore `/var/lib/vorpal/` state
- **No runbooks:** No operational runbooks for incident response
- **No SLOs/SLIs:** No defined service level objectives or indicators

---

## 9. Platform Support Matrix

| Platform | CI Tested | Release Binary | Container Image |
|----------|-----------|---------------|-----------------|
| macOS aarch64 (Apple Silicon) | Yes | Yes | No |
| macOS x86_64 (Intel) | Yes | Yes | No |
| Linux aarch64 | Yes | Yes | Yes |
| Linux x86_64 | Yes | Yes | Yes |

CI validates all 4 platforms for every PR. Static linking is enforced -- CI verifies no
non-system dynamic dependencies exist in release binaries.

---

## 10. Secrets and Configuration

### 10.1 GitHub Repository Secrets

| Secret | Purpose |
|--------|---------|
| `AWS_ACCESS_KEY_ID` | S3 registry access for integration tests |
| `AWS_SECRET_ACCESS_KEY` | S3 registry access for integration tests |
| `ALTF4LLC_GITHUB_APP_ID` | GitHub App for nightly tag creation |
| `ALTF4LLC_GITHUB_APP_PRIVATE_KEY` | GitHub App for nightly tag creation |
| `CARGO_REGISTRY_TOKEN` | crates.io publish |
| `DOCKERHUB_TOKEN` | DockerHub image push |
| `DOCKERHUB_USERNAME` | DockerHub image push |

### 10.2 GitHub Repository Variables

| Variable | Purpose |
|----------|---------|
| `AWS_DEFAULT_REGION` | AWS region for S3 registry |

---

## 11. Summary of Operational Gaps

The project has a solid CI/CD pipeline with cross-platform testing and automated releases.
The primary gaps are in production operational tooling:

1. **No monitoring/observability stack** -- logging exists but no metrics, tracing, or alerting
2. **No operational runbooks** -- no documented procedures for common failure scenarios
3. **No backup/restore tooling** -- filesystem store has no backup strategy
4. **No deployment orchestration** -- single-binary copy model with no staged rollout
5. **No audit trail** -- no record of builds, authentication events, or administrative actions
6. **No SLOs/SLIs** -- no defined reliability targets
7. **No rate limiting or resource quotas** -- services have no built-in throttling

These gaps are consistent with the project's experimental maturity level and are expected
to be addressed as the project moves toward production readiness.
