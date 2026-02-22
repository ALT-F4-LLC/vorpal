# Operations Specification

This document describes how Vorpal is built, deployed, and operated based on what actually exists
in the codebase.

---

## 1. CI/CD Pipeline

### Primary Workflow (`.github/workflows/vorpal.yaml`)

The main CI pipeline triggers on pull requests and pushes to `main` or any tag. It uses
concurrency groups with `cancel-in-progress: true` to avoid redundant runs.

**Pipeline stages (sequential):**

1. **vendor** -- Runs on a 4-runner matrix (`macos-latest`, `macos-latest-large`,
   `ubuntu-latest`, `ubuntu-latest-arm64`). Checks out code, restores cached `target/` and
   `vendor/` directories keyed by `{arch}-{os}-{Cargo.lock hash}`, bootstraps the dev
   environment via `./script/dev.sh`, vendors Cargo dependencies, and runs `cargo check
   --offline --release`.

2. **code-quality** (depends on vendor) -- Runs on `macos-latest` only. Executes `cargo fmt
   --all --check` and `cargo clippy --offline --release -- --deny warnings`.

3. **build** (depends on code-quality) -- Runs on the same 4-runner matrix. Executes `cargo
   build --offline --release`, `cargo test --offline --release`, and packages the binary into
   `dist/vorpal-{arch}-{os}.tar.gz`. Uploads the tarball as a GitHub Actions artifact named
   `vorpal-dist-{arch}-{os}`.

4. **test** (depends on build) -- Runs on the same 4-runner matrix. Downloads the dist
   artifact, extracts it, and uses `ALT-F4-LLC/setup-vorpal-action@main` to bootstrap a
   Vorpal environment with an S3 registry backend (`altf4llc-vorpal-registry` bucket). Builds
   multiple Vorpal artifacts (`vorpal`, `vorpal-container-image`, `vorpal-job`,
   `vorpal-process`, `vorpal-shell`, `vorpal-user`) using both the Rust config (`Vorpal.toml`)
   and Go config (`Vorpal.go.toml`), verifying that outputs are identical across SDK
   implementations. Ubuntu runners install additional dependencies via
   `./script/dev/debian.sh`. Uploads `Vorpal.lock` as a build artifact.

5. **container-image** (depends on test, tag pushes only) -- Runs on `ubuntu-latest` and
   `ubuntu-latest-arm64`. Builds the container image via Vorpal's own build system, loads it
   into Docker, tags it as `docker.io/altf4llc/vorpal:{tag}-{amd64|arm64}`, and pushes to
   Docker Hub using `DOCKERHUB_TOKEN` and `DOCKERHUB_USERNAME` secrets.

6. **container-image-manifest** (depends on container-image, tag pushes only) -- Runs on
   `ubuntu-latest`. Creates and pushes a multi-arch Docker manifest combining the `amd64` and
   `arm64` images under `docker.io/altf4llc/vorpal:{tag}`.

7. **release** (depends on test, tag pushes only) -- Runs on `ubuntu-latest`. Downloads all
   four dist artifacts and creates a GitHub Release via `softprops/action-gh-release@v2` with
   pre-release flag. The release includes tarballs for all four platform combinations:
   `aarch64-darwin`, `aarch64-linux`, `x86_64-darwin`, `x86_64-linux`. Also generates build
   provenance attestations using `actions/attest-build-provenance@v3`.

### Nightly Workflow (`.github/workflows/vorpal-nightly.yaml`)

Runs daily at 08:00 UTC via cron schedule (also supports `workflow_dispatch`). Uses a GitHub
App token to delete the existing `nightly` release and tag, then recreates the `nightly` tag
pointing to the latest `main` commit SHA. This triggers the primary workflow (which fires on
tag push), producing a nightly release.

### CI Secrets and Variables

| Secret/Variable | Purpose |
|---|---|
| `AWS_ACCESS_KEY_ID` | S3 registry backend authentication |
| `AWS_SECRET_ACCESS_KEY` | S3 registry backend authentication |
| `AWS_DEFAULT_REGION` (var) | AWS region for S3 |
| `DOCKERHUB_TOKEN` | Docker Hub push authentication |
| `DOCKERHUB_USERNAME` | Docker Hub push authentication |
| `ALTF4LLC_GITHUB_APP_ID` | GitHub App for nightly tag management |
| `ALTF4LLC_GITHUB_APP_PRIVATE_KEY` | GitHub App private key |

### Dependency Management

Renovate is configured (`.github/renovate.json`) with:
- Base config extending `config:base`
- All commits prefixed with `chore` semantic type
- Weekly lock file maintenance
- Ignores `cli/src/command/template/**` paths

---

## 2. Build System

### Makefile Targets

| Target | Description |
|---|---|
| `build` (default) | `cargo build` with optional `--offline --release` via `TARGET=release` |
| `check` | `cargo check` |
| `format` | `cargo fmt --all --check` |
| `lint` | `cargo clippy -- --deny warnings` |
| `test` | `cargo test` |
| `dist` | Packages binary into `dist/vorpal-{arch}-{os}.tar.gz` |
| `vendor` | `cargo vendor --versioned-dirs vendor/` |
| `.cargo` | Creates `.cargo/config.toml` pointing to vendored sources |
| `clean` | Removes `target/`, `.cargo/`, `dist/`, `vendor/` |
| `generate` | Regenerates Go SDK protobuf code from `sdk/rust/api/*.proto` |
| `vorpal` | Runs `vorpal build` via cargo |
| `vorpal-start` | Runs `vorpal system services start` via cargo |

### Development Environment Bootstrap

`./script/dev.sh` is the canonical entry point for development environment setup:

1. Detects Linux distribution (Debian/Ubuntu or Arch) and installs system packages:
   - **Debian/Ubuntu** (`script/dev/debian.sh`): `bubblewrap`, `build-essential`,
     `ca-certificates`, `curl`, `jq`, `rsync`, `unzip`, Docker (if missing)
   - **Arch** (`script/dev/arch.sh`): `docker`, `bubblewrap`, `ca-certificates`, `curl`, `unzip`
   - **macOS**: No system package installation (assumes Xcode CLI tools installed)
2. Creates `.env/bin/` directory for local tool binaries
3. Runs tool install scripts: `rustup.sh`, `protoc.sh`, `terraform.sh`
4. Adds `.env/bin` and `~/.cargo/bin` to `PATH`
5. Executes any command passed as arguments (e.g., `./script/dev.sh make build`)

### Rust Toolchain

Pinned via `rust-toolchain.toml`:
- Channel: `1.89.0`
- Components: `clippy`, `rust-analyzer`, `rustfmt`
- Profile: `minimal`
- Auto self-update: disabled

### Caching Strategy (CI)

Two cache layers keyed by `{arch}-{os}-{Cargo.lock hash}`:
- `target/` -- Cargo build cache (saved after vendor stage)
- `vendor/` -- Vendored crate sources (saved after vendor stage)

Both caches are restored at the start of each stage and saved at the end of the vendor stage
only. The Cargo.lock hash ensures caches invalidate when dependencies change.

---

## 3. Installation and Deployment

### End-User Installation (`script/install.sh`)

The install script supports both interactive and non-interactive modes (`-y`, `--yes`,
`VORPAL_NONINTERACTIVE=1`, or `CI=true`).

**Steps:**
1. Downloads the `nightly` release tarball from GitHub Releases for the current
   platform (`{arch}-{os}`)
2. Extracts to `~/.vorpal/bin/`
3. Creates system directories: `/var/lib/vorpal/{key,log,sandbox,store}` and
   `/var/lib/vorpal/store/artifact/{alias,archive,config,output}` (requires sudo)
4. Sets ownership to the current user
5. Generates TLS keypair via `vorpal system keys generate`
6. Configures a system service:
   - **macOS**: Creates a LaunchAgent plist at
     `~/Library/LaunchAgents/com.altf4llc.vorpal.plist` with `KeepAlive: true` and
     `RunAtLoad: true`. Logs to `/var/lib/vorpal/log/services.log`.
   - **Linux**: Creates a systemd unit at `/etc/systemd/system/vorpal.service` with
     `Restart=always` and `RestartSec=5`. Enables and starts the service.

### Service Management

The `vorpal system services start` command starts gRPC services. Default services:
`agent,registry,worker`.

**Transport modes:**
- **Unix Domain Socket** (default): Listens on `/var/lib/vorpal/vorpal.sock` (override via
  `VORPAL_SOCKET_PATH` env var). Socket permissions set to `0o660`. Uses an advisory lock
  file at `/var/lib/vorpal/vorpal.lock` to prevent TOCTOU races.
- **TCP** (with `--port` or `--tls`): Listens on `[::]:{port}` (default `23151` when TLS
  enabled).

**Optional capabilities:**
- `--tls`: Enables TLS using certificates from `/var/lib/vorpal/key/`
- `--health-check`: Enables a separate plaintext TCP health-check endpoint (default port
  `23152`) using the gRPC health protocol
- `--issuer`: Enables OIDC authentication for registry and worker services
- `--registry-backend`: Storage backend (`local` or `s3`)
- `--archive-check-cache-ttl`: TTL in seconds for caching archive check results (default 300)

**Signal handling:** Graceful shutdown on SIGINT and SIGTERM. Socket file is cleaned up on
shutdown; lock file is left on disk (advisory lock released on process exit).

### Container Image

Published to Docker Hub at `docker.io/altf4llc/vorpal:{tag}` as a multi-architecture image
(amd64 + arm64). Built via Vorpal's own build system (not a Dockerfile). Container images are
only published on tag pushes.

---

## 4. Infrastructure (Terraform)

### Overview

Terraform configuration in `terraform/` provisions development/testing infrastructure on AWS.
Uses Terraform `1.14.3` (installed via `script/dev/terraform.sh`).

### Providers

- `hashicorp/aws` v6.31.0
- `keycloak/keycloak` v5.6.0

### AWS Infrastructure (`terraform/module/workers/`)

Provisions a development VPC and EC2 instances for multi-platform testing:

| Resource | Type | Instance Type | Purpose |
|---|---|---|---|
| VPC (`vorpal-dev`) | VPC | -- | `10.42.0.0/16` CIDR, single AZ, public subnets only, no NAT gateway |
| `vorpal-dev-registry` | EC2 (Ubuntu 24.04 arm64) | `t4g.large` | Registry service host |
| `vorpal-dev-worker-aarch64-linux` | EC2 (Ubuntu 24.04 arm64) | `t4g.large` | ARM64 Linux worker |
| `vorpal-dev-worker-x8664-linux` | EC2 (Ubuntu 24.04 x86_64) | `t3a.large` | x86_64 Linux worker |
| `vorpal-dev-worker-aarch64-darwin` | EC2 (macOS Sequoia arm64) | `mac2.metal` | ARM64 macOS worker (optional, Dedicated Host) |
| `vorpal-dev-worker-x8664-darwin` | EC2 (macOS Sequoia x86_64) | `mac1.metal` | x86_64 macOS worker (optional, Dedicated Host) |

All instances have 100GB root EBS volumes. macOS instances are gated behind
`var.create_mac_instances` (default: `false`) due to Dedicated Host costs. SSH key pair is
auto-generated and the private key is stored in AWS SSM Parameter Store as a SecureString at
`/vorpal-dev/private-key`. Security group allows all inbound from a configurable CIDR
(`var.ssh_ingress_cidr`, default `0.0.0.0/0`).

### Keycloak Configuration (`terraform/module/keycloak/`)

Provisions the Vorpal OIDC realm in Keycloak:
- Realm: `vorpal`
- Clients: `cli` (public, device auth grant), `archive` (confidential, token exchange),
  `artifact` (confidential, token exchange), `worker` (confidential, token exchange, service
  accounts)
- Client scopes: `archive`, `artifact`, `worker` (audience and role mappers)
- Default admin user: `admin@localhost` / `password` (development only)
- Roles: `archive:check/push/pull`, `artifact:get/get-alias/store`,
  `worker:build-artifact`

---

## 5. Local Development Environment

### Lima VMs

Lima configuration (`lima.yaml`) provides Linux development VMs on macOS:
- Base image: Debian 12 (Bookworm) for both x86_64 and aarch64
- Mount types: no 9p (uses default reverse sshfs)
- Mounts: home directory (read-only) and `/tmp/lima` (writable)

Makefile targets:
- `make lima`: Creates, provisions, and starts a Lima VM
- `make lima-sync`: Syncs project files into the VM
- `make lima-vorpal`: Runs a Vorpal build inside the VM
- `make lima-vorpal-start`: Starts Vorpal services inside the VM
- `make lima-clean`: Stops and deletes the VM

### Docker Compose

`docker-compose.yaml` runs Keycloak for local OIDC development:
- Image: `quay.io/keycloak/keycloak:26.5.2`
- Mode: `start-dev`
- Admin credentials: `admin` / `password`
- Exposed on `127.0.0.1:8080`

---

## 6. Data Storage and Directory Layout

### Root Directory: `/var/lib/vorpal/`

```
/var/lib/vorpal/
  key/                          # TLS and signing keys
    ca.key.pem                  # CA private key (RSA, PKCS8)
    ca.pem                      # CA self-signed certificate
    service.key.pem             # Service private key
    service.pem                 # Service certificate (signed by CA)
    service.public.pem          # Service public key
    service.secret              # Service secret (UUID v7)
    credentials.json            # OAuth2 credentials (access/refresh tokens)
  log/                          # Log directory (used by LaunchAgent/systemd)
    services.log                # Service stdout/stderr
  sandbox/                      # Temporary build sandboxes (UUID-named)
  store/
    artifact/
      alias/{namespace}/{system}/{name}/{tag}  # Symlinks to output digests
      archive/{namespace}/{digest}.tar.zst     # Compressed build archives
      config/{namespace}/{digest}.json         # Artifact configuration
      output/{namespace}/{digest}/             # Extracted build outputs
      output/{namespace}/{digest}.lock.json    # Output lock files
  vorpal.sock                   # Unix domain socket (runtime)
  vorpal.lock                   # Advisory lock file (runtime)
```

### Registry Backends

- **local**: Reads/writes directly to the `/var/lib/vorpal/store/` directory tree
- **s3**: Uses an S3 bucket (configured via `--registry-backend-s3-bucket`). Supports path-style
  access via `--registry-backend-s3-force-path-style` (useful for S3-compatible services like
  MinIO).

---

## 7. Logging and Observability

### Logging

Vorpal uses the `tracing` crate with a `tracing-subscriber` `FmtSubscriber`:
- Output: stderr
- Default level: `INFO` (configurable via `--level` CLI flag)
- Format: No timestamps in default mode; file and line numbers enabled at `DEBUG`/`TRACE` levels
- No structured log aggregation or external log shipping configured

### Health Checks

gRPC health checking protocol (`tonic-health`) is available when `--health-check` is passed to
`vorpal system services start`. Runs on a separate plaintext TCP port (default 23152). Reports
per-service health status for each registered gRPC service.

### Gaps

- **No metrics collection**: No Prometheus, StatsD, or other metrics instrumentation exists
- **No distributed tracing**: No OpenTelemetry or Jaeger integration
- **No alerting**: No alerting rules or PagerDuty/OpsGenie integration
- **No dashboards**: No Grafana or equivalent monitoring dashboards
- **No centralized logging**: Logs go to stderr/file only; no log shipping to external services
- **No uptime monitoring**: No synthetic probes or external health checks

---

## 8. Key Management

Keys are generated via `vorpal system keys generate` and stored under `/var/lib/vorpal/key/`:

1. **CA key pair** (`ca.key.pem`): RSA key using `PKCS_RSA_SHA256`. Generated once; subsequent
   runs skip if file exists.
2. **CA certificate** (`ca.pem`): Self-signed X.509 cert with `IsCa`, DN includes `C=US`,
   `O=Vorpal`. Key usages: DigitalSignature, KeyCertSign, CrlSign.
3. **Service key pair** (`service.key.pem`, `service.public.pem`): RSA key for the service.
4. **Service certificate** (`service.pem`): Signed by the CA. SAN: `localhost`. Key usage:
   DigitalSignature. Extended key usage: ServerAuth.
5. **Service secret** (`service.secret`): UUID v7 string.

All key generation is idempotent -- files are only created if they do not already exist.

---

## 9. System Maintenance

### Pruning (`vorpal system prune`)

Clears local store data with granular control:
- `--all`: Prune everything
- `--artifact-aliases`: Clear alias symlinks
- `--artifact-archives`: Clear compressed archives
- `--artifact-configs`: Clear artifact configurations
- `--artifact-outputs`: Clear extracted outputs
- `--sandboxes`: Clear build sandboxes

Reports space freed for each category and total.

### Linux Rootfs Slimming (`script/linux-vorpal-slim.sh`)

A comprehensive script for reducing Vorpal-built Linux rootfs images from ~2.9GB to
~600-700MB. Removes development tools, documentation, locales, and other non-runtime files
across 13 configurable sections. Features dry-run mode (default), backup creation, section
selection, and aggressive mode (binary stripping). Verifies essential files after slimming.

---

## 10. Release Process

### Versioned Releases

1. A Git tag is pushed (e.g., `v0.1.0`)
2. The primary CI workflow triggers on the tag push
3. All stages run: vendor, code-quality, build, test
4. On success: `container-image`, `container-image-manifest`, and `release` stages execute
5. GitHub Release created with pre-release flag, containing platform tarballs
6. Docker multi-arch image pushed to Docker Hub
7. Build provenance attestations generated

### Nightly Releases

1. Nightly cron job (08:00 UTC) or manual `workflow_dispatch`
2. Deletes existing `nightly` release and tag
3. Creates new `nightly` tag pointing to latest `main` commit
4. Tag push triggers the full primary workflow, producing a nightly release

### Rollback

No automated rollback procedures exist. Rollback options:
- **GitHub Release**: Previous releases remain available; users can pin to a specific version
  in the install script by changing `INSTALL_VERSION`
- **Docker Hub**: Previous tagged images remain available
- **Infrastructure**: Terraform state can be used to revert infrastructure changes
- **Services**: LaunchAgent/systemd restart policies will restart with whatever binary is
  installed

---

## 11. Supported Platforms

| Platform | Architecture | CI Matrix | Worker Infra | Notes |
|---|---|---|---|---|
| macOS | aarch64 (Apple Silicon) | `macos-latest` | `mac2.metal` (optional) | Primary dev platform |
| macOS | x86_64 (Intel) | `macos-latest-large` | `mac1.metal` (optional) | |
| Linux | aarch64 | `ubuntu-latest-arm64` | `t4g.large` | |
| Linux | x86_64 | `ubuntu-latest` | `t3a.large` | |

Container images are Linux-only (amd64 + arm64).

---

## 12. Environment Variables

| Variable | Purpose | Default |
|---|---|---|
| `VORPAL_SOCKET_PATH` | Override Unix socket path | `/var/lib/vorpal/vorpal.sock` |
| `VORPAL_NONINTERACTIVE` | Skip install script prompts | `0` |
| `CI` | Enables non-interactive mode | unset |
| `AWS_ACCESS_KEY_ID` | S3 backend authentication | -- |
| `AWS_SECRET_ACCESS_KEY` | S3 backend authentication | -- |
| `AWS_DEFAULT_REGION` | S3 backend region | -- |
