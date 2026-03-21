---
project: "vorpal"
maturity: experimental
last_updated: "2026-03-20"
updated_by: "@staff-engineer"
scope: "Authentication, authorization, secret management, and trust boundaries"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - operations.md
---

# Security

## Overview

Vorpal's security model covers TLS transport encryption, OIDC-based authentication for registry and worker services, content integrity via SHA-256 digests, RSA-based secret encryption for build steps, and namespace-based authorization.

## Authentication

### OIDC Authentication

Vorpal supports OIDC (OpenID Connect) authentication for protecting registry and worker services. The implementation lives in `cli/src/command/start/auth.rs`.

**Server-side validation (`OidcValidator`):**
- Discovers the OIDC provider via `/.well-known/openid-configuration`
- Fetches and caches JWKS (JSON Web Key Set) for token signature verification
- Validates RS256-signed JWTs with issuer, audience, expiry, and not-before checks
- Automatically refreshes JWKS on key-not-found (handles rolling keys)
- Normalizes issuer URLs (trailing slash handling for Auth0 compatibility)
- Implemented as a tonic gRPC interceptor that extracts and validates `authorization` metadata

**Client-side authentication:**
- `vorpal login` implements OAuth2 Device Authorization Grant flow
- Short-lived tokens are stored in a credentials file at the key service credentials path
- Credentials are scoped per-issuer and per-registry
- Client credentials flow (`exchange_client_credentials`) supports service-to-service auth for worker service

**When auth is disabled:**
- If no `--issuer` flag is provided to `system services start`, registry and worker services run without authentication
- Agent service never requires authentication (it is a local service)

### Keycloak Integration

A local Keycloak instance is available via `docker-compose.yaml` for development:
- Image: `quay.io/keycloak/keycloak:26.5.5`
- Default admin credentials: `admin`/`password` (development only)
- Bound to `127.0.0.1:8080` (localhost only)
- Terraform configuration in `terraform/module/keycloak/` manages realm setup

## Authorization

### Namespace Permissions

Claims include an optional `namespaces` field mapping namespace names to permission arrays:

```rust
pub namespaces: Option<HashMap<String, Vec<String>>>
```

- Exact namespace match: checks if the user has a specific permission for a given namespace
- Wildcard admin: `"*"` namespace grants permissions across all namespaces
- The `require_namespace_permission()` helper enforces permissions in gRPC handlers, returning 403 on failure

### Current Authorization Gaps

- No RBAC role definitions in the codebase -- namespace permissions are expected in JWT claims
- No authorization on Agent service (by design -- it is local)
- Audit logging extracts user context (`get_user_context`) but integration with structured audit logs is not implemented

## Transport Security

### TLS

- TLS is optional and disabled by default (Unix Domain Socket mode)
- When enabled (`--tls` flag), the server loads certificate and private key from the key service paths under `/var/lib/vorpal/key/`
- `vorpal system keys generate` creates self-signed TLS certificates using `rcgen` with `aws_lc_rs` backend
- The install script generates TLS keys automatically during installation
- UDS mode restricts access to file-system permissions (socket set to `0660`)

### gRPC Transport

- Default: Unix Domain Socket at `/var/lib/vorpal/vorpal.sock` (overridable via `VORPAL_SOCKET_PATH` env var)
- TCP mode: available via `--port` flag
- Health check endpoint: separate plaintext TCP listener (port 23152 by default)

## Secret Management

### Build Step Secrets

Secrets in artifact build steps are encrypted using RSA public key encryption:
- Public key stored at the key service public path
- Encryption handled by `cli/src/command/store/notary.rs`
- Secrets are encrypted during artifact preparation in the Agent service
- The `ArtifactStepSecret` protobuf message carries encrypted values

### Environment Variables and Credential Storage

- No `.env` files are used in the project
- Credentials are stored at the key service credentials path as JSON
- AWS credentials for S3 registry backend are provided via standard AWS environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`) in CI
- Docker Hub credentials are stored as GitHub Actions secrets

### Sensitive Paths

Key filesystem paths related to security:
- `/var/lib/vorpal/key/` -- TLS certificates and private keys
- Key credentials path -- OAuth2 tokens (per-issuer, per-registry)
- `/var/lib/vorpal/vorpal.sock` -- UDS socket (permissions: `0660`)
- Lock file -- advisory lock preventing multiple instances

## Content Integrity

- All artifacts and sources are identified by SHA-256 content digests
- Source digest verification: computed digest must match locked digest (unless `--unlock` is used)
- Artifact digest: SHA-256 of the JSON-serialized artifact protobuf
- Source archives are compressed with zstd before storage
- CI verifies no non-system dynamic library dependencies are linked (prevents supply chain injection via dylib)

## Trust Boundaries

```
┌─────────────────────────────────────────────────┐
│ Local Machine (trusted)                          │
│   CLI ◄──UDS──► Agent Service                    │
│                    │                             │
│         (no auth required)                       │
└────────────┬────────────────────────────────────┘
             │ TLS + OIDC Bearer Token
             ▼
┌─────────────────────────────────────────────────┐
│ Remote Registry (semi-trusted)                   │
│   Archive Service ◄──► S3 Backend               │
│   Artifact Service ◄──► S3 Backend              │
│   Worker Service                                │
│                                                 │
│   (namespace-scoped permissions)                │
└─────────────────────────────────────────────────┘
```

## CI/CD Security

- GitHub Actions workflows use `id-token: write` permission for build provenance attestation
- `actions/attest-build-provenance@v4` creates SLSA provenance for release binaries
- Rust SDK published to crates.io with `CARGO_REGISTRY_TOKEN` secret
- TypeScript SDK published to npm with provenance (`--provenance` flag) -- uses `id-token: write`
- GitHub App token (not PAT) used for nightly release automation
- Renovate bot manages dependency updates with conservative automerge policies (3-day minimum release age for production deps)

## Gaps and Areas for Improvement

- No rate limiting on authentication endpoints
- No token refresh logic for stored credentials (TODO comment in login flow)
- JWKS refresh happens synchronously via `block_in_place` in the interceptor -- could be a bottleneck under high load
- No credential rotation automation
- No explicit secret scanning or pre-commit hooks for secret detection
- Keycloak development credentials are hardcoded in `docker-compose.yaml` (acceptable for local dev)
- No CSP or security headers -- not applicable (gRPC, not HTTP/web)
