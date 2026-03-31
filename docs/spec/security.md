---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "Security posture: authentication, authorization, cryptography, secrets, build isolation, supply chain, and trust boundaries"
owner: "@staff-engineer"
dependencies:
  - architecture.md
---

# Security Specification

## 1. Overview

Vorpal is a build system and artifact registry with a multi-service gRPC architecture (agent, registry, worker). Security concerns span: TLS/mTLS for transport, OIDC-based authentication and authorization, build-time sandboxing, secret management, supply chain integrity, and infrastructure access control. Authentication is optional -- services run unauthenticated by default in local/development mode.

## 2. Authentication

### 2.1 OIDC Integration

Vorpal supports OIDC-based authentication via an external identity provider (Keycloak is the reference implementation, managed via Terraform in `terraform/module/keycloak/`).

**Server-side validation** (`cli/src/command/start/auth.rs`):

- `OidcValidator` performs OIDC discovery (`/.well-known/openid-configuration`) at startup
- Validates issuer match (normalized, trailing-slash tolerant for Auth0 compatibility)
- Fetches JWKS and caches keys in-memory with automatic refresh on `kid` miss (handles key rotation)
- Token validation enforces: RS256 algorithm, `kid` matching, audience, issuer, expiration (`exp`), and not-before (`nbf`)
- Claims are extracted and stashed in gRPC request extensions for downstream handlers

**Client-side flows**:

- **Device Authorization Grant** (`cli/src/command.rs`, `Command::Login`): The `vorpal login` command uses OAuth2 device code flow for interactive CLI authentication. Tokens (access + refresh) are stored in `/var/lib/vorpal/key/credentials.json`.
- **Client Credentials Grant** (`cli/src/command/start/auth.rs`, `exchange_client_credentials`): Workers use service-to-service OAuth2 client credentials flow to authenticate with registry services. Client ID and secret are passed via CLI flags (`--issuer-client-id`, `--issuer-client-secret`).
- **Token refresh** (`sdk/rust/src/context.rs`, `refresh_access_token`): The SDK automatically refreshes expired access tokens using stored refresh tokens before making registry calls. Tokens are refreshed when less than 5 minutes remain before expiration.

**Keycloak realm configuration** (`terraform/module/keycloak/local.tf`):

- Realm: `vorpal`
- Clients: `cli` (PUBLIC, device auth grant), `archive` (CONFIDENTIAL), `artifact` (CONFIDENTIAL), `worker` (CONFIDENTIAL, service accounts enabled)
- Client scopes: `archive`, `artifact`, `worker` -- each with audience protocol mapper and role protocol mapper
- Development user: `admin` / `password` (hardcoded in Terraform locals -- development only)

### 2.2 Authentication as Optional

Authentication is entirely opt-in. When `--issuer` is not provided to `vorpal system services start`, all gRPC services run without any interceptor, meaning any client can call any endpoint. The worker checks `request.extensions().get::<auth::Claims>().is_some()` before enforcing namespace permissions -- if no claims are present (no auth configured), the check is skipped entirely (`cli/src/command/start/worker.rs:964`).

**Gap**: There is no warning or log message when services start without authentication enabled. In production deployments, this could lead to accidentally running unauthenticated.

### 2.3 gRPC Interceptor

The auth interceptor (`cli/src/command/start/auth.rs`, `new_interceptor`) is synchronous (required by tonic's `Interceptor` trait) but wraps async OIDC validation via `tokio::task::block_in_place`. This is noted in a code comment as acceptable for current throughput but potentially worth replacing with a tower layer for high-throughput scenarios.

## 3. Authorization

### 3.1 Namespace-Based Permissions

Authorization is namespace-scoped. Claims include a `namespaces` field mapping namespace names to permission arrays.

**`Claims::has_namespace_permission`** (`cli/src/command/start/auth.rs:55-69`):
- Checks exact namespace match first
- Falls back to wildcard (`*`) namespace for admin access
- Permissions are string-based (e.g., `"write"`, `"read"`)

**`require_namespace_permission`** (`cli/src/command/start/auth.rs:405-423`):
- Extracts claims from gRPC request extensions
- Returns `UNAUTHENTICATED` if no claims found
- Returns `PERMISSION_DENIED` if namespace permission is missing

**Current enforcement points**:
- Worker `build_artifact`: requires `write` permission on the artifact namespace (`cli/src/command/start/worker.rs:966`)

**Gap**: The registry services (archive push/pull, artifact get/store) apply the OIDC interceptor at the service level when `--issuer` is configured (`cli/src/command/start.rs:212-235`), but individual RPC handlers do not perform namespace-level permission checks. The interceptor validates the token is valid but does not enforce what the caller is allowed to do within specific namespaces for registry operations.

### 3.2 Audit Logging

`get_user_context` (`cli/src/command/start/auth.rs:426-431`) extracts the `sub` claim for audit logging. Currently used in the worker's `build_artifact` to log which user initiated a build. Not systematically applied across all services.

## 4. Cryptography

### 4.1 TLS / mTLS

**Server TLS** (`cli/src/command/start.rs`, `new_tls_config`):
- Optional, enabled via `--tls` flag on `vorpal system services start`
- Uses service certificate and private key from `/var/lib/vorpal/key/service.pem` and `/var/lib/vorpal/key/service.key.pem`
- TLS implies TCP mode (default port 23151); without TLS, services default to Unix domain socket

**Client TLS** (`sdk/rust/src/context.rs`, `get_client_tls_config`):
- Automatically enabled for `https://` URIs
- Uses local CA certificate (`/var/lib/vorpal/key/ca.pem`) if present for custom CA trust
- Falls back to system native roots if no local CA exists
- No TLS for `http://` or `unix://` URIs

### 4.2 Key Generation

`vorpal system keys generate` (`cli/src/command/system/keys.rs`) produces a complete PKI:

| File | Algorithm | Purpose |
|------|-----------|---------|
| `ca.key.pem` | RSA (PKCS_RSA_SHA256 via `rcgen`) | CA private key |
| `ca.pem` | Self-signed X.509 | CA certificate (KeyCertSign, CrlSign, DigitalSignature) |
| `service.key.pem` | RSA (PKCS_RSA_SHA256) | Service private key |
| `service.public.pem` | RSA public key (PKCS8) | Service public key (used for secret encryption) |
| `service.pem` | X.509 signed by CA | Service certificate (SAN: localhost, ServerAuth EKU) |
| `service.secret` | UUID v7 | Service secret (opaque string) |

All keys are stored under `/var/lib/vorpal/key/`. Generation is idempotent -- each file is only created if it does not already exist.

**Observations**:
- Service certificate SAN is hardcoded to `localhost` only. Remote TLS connections to non-localhost addresses will fail certificate verification unless clients disable hostname verification.
- No certificate expiration is configured (rcgen defaults apply).
- No key rotation mechanism exists. Keys persist until manually deleted.
- The CA is unconstrained (`BasicConstraints::Unconstrained`) -- it can sign any number of subordinate CAs.

### 4.3 Notary (Secret Encryption)

`cli/src/command/store/notary.rs` provides RSA-based secret encryption:

- **Encrypt**: Uses `RsaPublicKey` with `Pkcs1v15Encrypt` padding, returns base64-encoded ciphertext
- **Decrypt**: Uses `RsaPrivateKey` with `Pkcs1v15Encrypt` padding

This is used in the worker's `run_step` to decrypt artifact step secrets before injecting them as environment variables during builds.

**Security note**: PKCS#1 v1.5 encryption padding is generally considered legacy. OAEP padding would be preferred for new implementations, though the risk is low here since the encryption is used for short-lived build secrets between trusted components.

### 4.4 Credential Storage

Credentials from `vorpal login` are stored at `/var/lib/vorpal/key/credentials.json` containing:
- Access tokens (JWT)
- Refresh tokens
- Client ID, audience, scopes, expiry metadata
- Registry-to-issuer mapping

**Gap**: The credentials file is stored in plaintext JSON on disk. File permissions are not explicitly set during write -- they inherit the process umask. The file sits alongside private keys in the same directory.

## 5. Build Isolation

### 5.1 Linux: bubblewrap (bwrap)

On Linux, build steps use bubblewrap (`bwrap`) for sandboxing (`sdk/rust/src/artifact/step.rs:60-218`):

- `--unshare-all`: Unshares all namespaces (PID, network, mount, UTS, IPC, user)
- `--share-net`: Re-shares network (builds can access the network)
- `--clearenv`: Clears environment, then selectively sets variables
- `--gid 1000 --uid 1000`: Runs as unprivileged user inside the sandbox
- `--dev /dev`, `--proc /proc`, `--tmpfs /tmp`: Minimal device/proc/tmp
- Root filesystem is read-only bound (`--ro-bind`) from a Vorpal-built Linux rootfs
- Artifact dependencies are read-only bound
- Only `$VORPAL_OUTPUT` and `$VORPAL_WORKSPACE` are writable (bind-mounted)

### 5.2 macOS: No Sandbox

On macOS, build steps run as plain bash processes with no isolation (`sdk/rust/src/artifact/step.rs:234`). The build script executes directly via `Command::new` with full access to the host filesystem and network.

**Gap**: macOS builds have no sandboxing whatsoever. Any build script has full access to the user's filesystem, environment, and network.

### 5.3 Build Environment Variables

Build steps receive controlled environment variables:
- `VORPAL_OUTPUT`: Path to the artifact output directory (writable)
- `VORPAL_WORKSPACE`: Path to the workspace directory (writable)
- `VORPAL_ARTIFACT_<digest>`: Paths to dependency artifacts (read-only in bwrap, writable on macOS)
- `HOME` is set to `$VORPAL_WORKSPACE`
- `PATH` is constructed from artifact `bin/` directories plus standard system paths
- Secrets are decrypted and injected as environment variables by name

## 6. Supply Chain Security

### 6.1 Build Provenance

Binary releases use `actions/attest-build-provenance@v4` in the CI workflow (`vorpal.yaml:335-341`) with:
- `id-token: write` permission for Sigstore
- Subject paths cover all four platform binaries

**Gap**: No SHA-256 checksums are published alongside release tarballs. The installer (`script/install.sh`) does not verify download integrity beyond running `vorpal --version` after extraction.

### 6.2 Artifact Integrity

Artifacts are content-addressed by SHA-256 digest of their JSON configuration. The digest serves as both identifier and integrity check for the artifact definition. However, the archive contents (the actual built output) are not independently verified against a digest after download -- the archive is unpacked directly.

### 6.3 Dependency Management

- **Renovate** (`.github/renovate.json`): Automated dependency updates with tiered automerge policy:
  - GitHub Actions: minor/patch automerge
  - Production deps (Rust, Go, TypeScript): patch automerge with 3-day release age gate; minor automerge only for stable (>= 1.0) crates
  - Terraform providers: no automerge
  - Lock file maintenance: weekly, automerge
- **Cargo vendor**: Dependencies are vendored (Cargo workspace uses vendored deps via `script/dev.sh`)
- **TLS**: `reqwest` is configured with `rustls-tls` feature (no OpenSSL dependency). The CLI uses `ring` as the default crypto provider.

### 6.4 SDK Publishing

- **Rust SDK** (`crates.io`): Uses `CARGO_REGISTRY_TOKEN` secret (legacy token)
- **TypeScript SDK** (`npmjs.com`): Uses `NPM_TOKEN` secret with `NODE_AUTH_TOKEN` env var. The `id-token: write` permission is declared but not consumed (no `--provenance` flag). A TDD exists for migrating to OIDC trusted publishing (`docs/tdd/npm-oidc-trusted-publishing.md`).
- **Container images** (Docker Hub): Uses `DOCKERHUB_TOKEN` and `DOCKERHUB_USERNAME` secrets

## 7. Network Security

### 7.1 Transport Modes

| Mode | Transport | Authentication | Encryption |
|------|-----------|----------------|------------|
| Default (no flags) | Unix domain socket | None | N/A (local IPC) |
| `--port <N>` | TCP plaintext | Optional OIDC | None |
| `--tls` | TCP with TLS | Optional OIDC | TLS (rustls) |

### 7.2 Unix Domain Socket Security

When running in UDS mode (`cli/src/command/start.rs`):
- Socket permissions are set to `0o660` (owner + group read/write)
- Advisory file lock prevents multiple instances (`fs4::FileExt::try_lock_exclusive`)
- Stale socket detection: attempts connection before removing existing socket file
- Permission denied on existing socket is treated as a hard error (may belong to another user)

### 7.3 Health Check Endpoint

When enabled via `--health-check`, a separate plaintext TCP listener runs on port 23152 (default). This endpoint has no TLS and no authentication -- it serves only gRPC health check responses. The port must differ from the main service port.

## 8. Infrastructure Security

### 8.1 AWS Infrastructure (Terraform)

The `terraform/module/workers/` module provisions dev infrastructure:

- VPC with public subnets only (no private subnets, no NAT gateway)
- Security group allows all ingress from `var.ssh_ingress_cidr` and all egress
- EC2 instances (registry + workers across 4 platforms) with public IP addresses
- SSH key pair generated by Terraform, private key stored in AWS SSM Parameter Store as SecureString
- All instances on public subnets with direct internet access

**Observations**: This is development infrastructure. The security group rule `ingress_rules = ["all-all"]` allows all ports from the configured CIDR, not just SSH. All instances are publicly accessible.

### 8.2 Local Development (Docker Compose)

Keycloak runs in `start-dev` mode with hardcoded admin credentials (`admin`/`password`), bound to `127.0.0.1:8080`. This is development-only configuration.

## 9. CI/CD Security

### 9.1 GitHub Actions

- Workflow permissions use least-privilege per job (e.g., `contents: read` for test, `contents: write` + `attestations: write` + `id-token: write` for release)
- Concurrency groups prevent parallel runs on the same branch/PR
- Secrets used: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`, `DOCKERHUB_TOKEN`, `DOCKERHUB_USERNAME`
- Build verification: Static linking is verified on both macOS (`otool -L`) and Linux (`ldd`) to ensure no non-system dynamic dependencies leak into release binaries

### 9.2 Branch Protection

No branch protection rules are visible in the repository files. This would need to be verified in GitHub settings.

## 10. Known Gaps and Risks

| # | Gap | Severity | Area |
|---|-----|----------|------|
| 1 | No warning when services start without authentication | Medium | Authentication |
| 2 | Registry RPC handlers lack namespace-level permission checks | High | Authorization |
| 3 | macOS builds have no sandboxing | Medium | Build Isolation |
| 4 | Credential file permissions not explicitly set | Medium | Credential Storage |
| 5 | Service certificate SAN hardcoded to `localhost` | Medium | TLS |
| 6 | No certificate expiration or rotation mechanism | Low | TLS |
| 7 | No SHA-256 checksums published for release artifacts | Medium | Supply Chain |
| 8 | NPM publish lacks `--provenance` flag (TDD exists) | Low | Supply Chain |
| 9 | Cargo publish uses legacy token (no OIDC) | Low | Supply Chain |
| 10 | Archive contents not integrity-verified after download | Medium | Artifact Integrity |
| 11 | PKCS#1 v1.5 encryption padding in notary (legacy) | Low | Cryptography |
| 12 | Dev Terraform security group allows all ports, not just SSH | Low | Infrastructure |
| 13 | No rate limiting on gRPC endpoints | Low | DoS Protection |
| 14 | `block_in_place` in auth interceptor could block tokio runtime under load | Low | Performance/Security |
