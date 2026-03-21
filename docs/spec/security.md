---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Authentication, authorization, cryptographic operations, secret management, and transport security in the Vorpal build system"
owner: "@staff-engineer"
dependencies:
  - architecture.md
---

# Security Specification

## 1. Overview

Vorpal is a build system with a client/server architecture comprising an Agent, Registry (Archive + Artifact services), Worker, and SDK. Security features are opt-in: the system defaults to unauthenticated, plaintext communication and must be explicitly configured for TLS and OIDC-based authentication.

## 2. Authentication

### 2.1 OIDC / OAuth2 Integration

Vorpal supports OpenID Connect (OIDC) authentication via an external identity provider. The reference deployment uses Keycloak (v26.5.5), provisioned via Terraform in `terraform/module/keycloak/`.

**Client-side authentication (CLI login):**

- Implemented via the OAuth2 Device Authorization Grant flow (`cli/src/command.rs:523-645`)
- The CLI (`vorpal login`) performs OIDC discovery against the issuer's `.well-known/openid-configuration` endpoint, then initiates the device code flow
- Users authenticate out-of-band (browser) and the CLI polls for the resulting token
- Obtained tokens (access token, refresh token, expiry, scopes) are stored in a local credentials file at `/var/lib/vorpal/key/credentials.json`
- The `cli` OIDC client is configured as `PUBLIC` access type with device authorization grant enabled

**Server-side token validation:**

- Implemented in `cli/src/command/start/auth.rs` via the `OidcValidator` struct
- On startup, the server discovers the issuer's JWKS URI and fetches the JSON Web Key Set
- JWT validation enforces: RS256 algorithm, `kid` matching, audience (`aud`), issuer (`iss`), expiration (`exp`), and not-before (`nbf`)
- JWKS are cached in-memory with automatic refresh on `kid` miss (handles key rotation)
- Issuer comparison is normalized (trailing slash tolerance) for Auth0 compatibility
- Validation is applied as a gRPC interceptor using `tonic::Request` — runs synchronously via `block_in_place` for simplicity

**Service-to-service authentication (worker):**

- The Worker service uses OAuth2 Client Credentials flow to obtain tokens for calling the Registry (Archive and Artifact services)
- Credentials (`issuer_client_id`, `issuer_client_secret`) are passed via CLI flags — not stored in files
- Separate scoped tokens are obtained for archive operations (`read:archive write:archive`) and artifact operations (`read:artifact write:artifact`)
- The `worker` OIDC client is configured as `CONFIDENTIAL` with service accounts enabled

**Token refresh:**

- Client-side tokens are automatically refreshed when within 5 minutes of expiration (`sdk/rust/src/context.rs:700-760`)
- Refresh uses the stored refresh token; if absent, the user is prompted to re-login
- Updated credentials are persisted back to the credentials file

### 2.2 Authentication is Optional

Authentication is **not enforced by default**. When the `--issuer` flag is omitted from `vorpal system services start`, all gRPC services operate without authentication. The interceptor is only attached when an issuer is provided. This means:

- Local development operates fully unauthenticated
- Production deployments must explicitly enable auth via `--issuer`, `--issuer-audience`, `--issuer-client-id`, and `--issuer-client-secret` flags

**Gap:** There is no warning emitted when services start without authentication enabled.

## 3. Authorization

### 3.1 Namespace-based Permissions

Authorization is implemented via custom JWT claims (`cli/src/command/start/auth.rs:49-69`):

- A `namespaces` claim in the JWT maps namespace names to arrays of permission strings
- The `require_namespace_permission()` helper checks this claim before allowing gRPC operations
- Wildcard namespace (`*`) grants access to all namespaces for a given permission
- Permissions are checked per-operation: `read` for pulls/gets, `write` for pushes/stores/builds

**Services with authorization checks:**

| Service | Endpoint | Permission |
|---------|----------|------------|
| Archive | `pull` | `read` on namespace |
| Archive | `push` | None (no auth check on push) |
| Artifact | `get_artifact` | `read` on namespace |
| Artifact | `get_artifact_alias` | `read` on namespace |
| Artifact | `store_artifact` | `write` on namespace |
| Worker | `build_artifact` | `write` on namespace |

**Gap:** Archive `push` does not enforce authorization even when auth is enabled. The `check` endpoint also lacks authorization.

### 3.2 Authorization is Conditional

Authorization checks only execute when `Claims` are present in the gRPC request extensions. If authentication is disabled (no issuer configured), no claims are injected and all authorization checks are skipped. The check pattern is:

```
if request.extensions().get::<auth::Claims>().is_some() {
    require_namespace_permission(&request, &namespace, "write")?;
}
```

### 3.3 Audit Logging

User context extraction (`get_user_context`) is available for audit logging via the JWT `sub` claim. Audit log entries are written via `tracing::info!` at the point of authorization check. This provides basic who-did-what logging but is not structured for security audit purposes.

**Gap:** Audit logging is informational-level tracing, not a dedicated security audit log.

## 4. Transport Security

### 4.1 TLS Configuration

TLS is opt-in for the main gRPC listener, controlled by the `--tls` flag.

**Server-side TLS:**

- Uses `tonic::transport::ServerTlsConfig` with PEM-encoded certificate and private key
- Certificate and key are read from `/var/lib/vorpal/key/service.pem` and `/var/lib/vorpal/key/service.key.pem`
- TLS implies TCP mode (default port 23151); without TLS, the server may use Unix domain sockets

**Client-side TLS:**

- For `https://` URIs, the client uses `tonic::transport::ClientTlsConfig`
- If a CA certificate exists at `/var/lib/vorpal/key/ca.pem`, it is used for certificate verification
- If no local CA cert exists, system native root certificates are used (`with_native_roots()`)
- For `http://` and `unix://` URIs, TLS is not used
- HTTP client (reqwest) uses `rustls-tls` backend (not OpenSSL)

**Health check listener:**

- The health check endpoint (`--health-check`, port 23152) always runs in plaintext, even when the main listener uses TLS
- This is intentional for load balancer health probes but is worth noting

### 4.2 Unix Domain Socket Security

When no `--port` is specified and TLS is disabled, the server listens on a Unix domain socket:

- Default path: `/var/lib/vorpal/vorpal.sock` (overridable via `VORPAL_SOCKET_PATH`)
- Socket permissions are set to `0o660` (owner + group read/write)
- Advisory file locking prevents multiple instances (`/var/lib/vorpal/vorpal.lock`)
- Stale socket detection: attempts to connect before removing, checks for permission-denied (different user)

## 5. Cryptographic Operations

### 5.1 PKI / Certificate Generation

The `vorpal system keys generate` command (`cli/src/command/system/keys.rs`) creates a local PKI:

- **CA key pair:** RSA (PKCS_RSA_SHA256) via `rcgen`, stored at `/var/lib/vorpal/key/ca.key.pem` and `ca.pem`
- **Service certificate:** Signed by the local CA, SAN set to `localhost`, valid for server authentication
- **Service key pair:** RSA, stored at `/var/lib/vorpal/key/service.key.pem`, `service.pem`, `service.public.pem`
- **Service secret:** UUID v7, stored at `/var/lib/vorpal/key/service.secret`
- All key generation is idempotent (skips if file already exists)

**Gaps:**
- No certificate expiration / rotation mechanism
- No file permission restrictions on generated key files
- CA is unconstrained (`BasicConstraints::Unconstrained`) — the CA can sign any certificate, not just for the Vorpal domain
- Service certificate is hardcoded to `localhost` SAN — won't work for remote deployments without regeneration

### 5.2 Secret Encryption (Notary)

Build step secrets are encrypted at rest during transit through the agent/worker pipeline (`cli/src/command/store/notary.rs`):

- **Encryption:** RSA PKCS1v15 with the service public key, then Base64-encoded
- **Decryption:** Base64-decoded, then RSA PKCS1v15 with the service private key
- Secrets are encrypted by the Agent when preparing artifacts and decrypted by the Worker when executing build steps
- Decrypted secret values are injected as environment variables during build step execution

**Gaps:**
- PKCS1v15 padding is used instead of the recommended OAEP padding scheme
- Error handling uses `expect()` (panics) rather than returning errors gracefully
- No key size is specified — the key size depends on what `rcgen` generates for `PKCS_RSA_SHA256`

### 5.3 Content Integrity

- Artifact and source integrity is verified using SHA-256 digests (`cli/src/command/store/hashes.rs`)
- Source digest is computed from individual file hashes concatenated and re-hashed
- Artifact digest is computed from the JSON serialization of the artifact metadata
- Source digest verification can be bypassed with the `--unlock` flag
- Digests are used as content-addressable storage keys

## 6. Credential Storage

### 6.1 File Locations

All credentials and keys are stored under `/var/lib/vorpal/key/`:

| File | Contents | Format |
|------|----------|--------|
| `ca.pem` | CA certificate | PEM |
| `ca.key.pem` | CA private key | PEM |
| `service.pem` | Service certificate | PEM |
| `service.key.pem` | Service private key | PEM |
| `service.public.pem` | Service public key | PEM |
| `service.secret` | Service secret (UUID v7) | Plaintext |
| `credentials.json` | OAuth2 tokens (access, refresh) | JSON |

**Gaps:**
- No file permission enforcement on key material (keys are written with default umask)
- Credentials file contains plaintext access tokens and refresh tokens
- No encryption-at-rest for stored credentials
- The credentials file is a single JSON object keyed by issuer — multiple registries store all tokens in one file

### 6.2 Environment Variables

| Variable | Purpose |
|----------|---------|
| `VORPAL_SOCKET_PATH` | Override default Unix socket path |
| AWS SDK environment variables | Used by S3 backend for registry storage (standard AWS credential chain) |

Issuer secrets (`--issuer-client-secret`) are passed as CLI arguments, which are visible in process listings.

## 7. Build Sandbox Security

### 7.1 Process Isolation

Build steps are executed as child processes via `tokio::process::Command` (`cli/src/command/start/worker.rs:423-607`):

- Each build runs in a sandbox directory under `/var/lib/vorpal/sandbox/<uuid>/`
- Working directory is set to the sandbox workspace
- Environment variables are explicitly set (not inherited from parent)
- Scripts are written to disk and executed with `0o755` permissions
- Sandbox directories are cleaned up after build completion

**Gaps:**
- No filesystem-level isolation (no chroot, no containers, no namespaces)
- No resource limits (CPU, memory, disk, network)
- No network isolation — build steps can make arbitrary network requests
- Build steps run as the same user as the Vorpal server process
- Script execution allows arbitrary code execution by design — security depends entirely on trust in artifact definitions
- Environment variable expansion (`expand_env`) processes all environment variables including secrets, which could leak into logs if commands echo their arguments

### 7.2 Artifact Source Handling

- HTTP sources are downloaded and unpacked (gzip, bzip2, xz, zip) without additional validation beyond digest verification
- Archive unpacking (tar) does not enforce path traversal protection — relies on the `tokio-tar` crate's defaults
- Local sources are copied from the filesystem with path validation limited to existence checks

## 8. Keycloak Configuration (Development)

The reference Keycloak deployment (`docker-compose.yaml`, `terraform/module/keycloak/`) defines:

**OIDC Clients:**
- `cli`: Public client, device authorization grant, optional scopes for archive/artifact/worker
- `archive`: Confidential client, token exchange enabled, roles: `archive:check`, `archive:push`, `archive:pull`
- `artifact`: Confidential client, token exchange enabled, roles: `artifact:get`, `artifact:get-alias`, `artifact:store`
- `worker`: Confidential client, service accounts enabled, optional scopes for archive/artifact, roles: `worker:build-artifact`

**Development credentials:**
- Keycloak admin: `admin` / `password`
- Test user: `admin@localhost` / `password`
- Keycloak runs in `start-dev` mode (not production-hardened)
- Keycloak is bound to `127.0.0.1:8080` (localhost only)

**Gap:** The namespace-based permission model (`namespaces` claim) referenced in the code is not provisioned in the Keycloak Terraform configuration. The Terraform defines client roles (e.g., `archive:push`, `artifact:store`) but the code checks a `namespaces` JWT claim that is not mapped via any protocol mapper. This means the namespace authorization system is defined in code but not yet wired up to the identity provider.

## 9. Dependency Security Profile

Security-critical dependencies (from `Cargo.toml` files):

| Crate | Version | Purpose | Notes |
|-------|---------|---------|-------|
| `jsonwebtoken` | 10.3.0 | JWT validation | Uses `aws_lc_rs` feature |
| `oauth2` | 5.0.0 | OAuth2 flows | Uses `reqwest` feature |
| `rcgen` | 0.14.7 | Certificate generation | Uses `aws_lc_rs`, `x509-parser` |
| `reqwest` | 0.12.28 | HTTP client | `rustls-tls` (no OpenSSL dependency) |
| `rsa` | 0.9.10 | RSA encrypt/decrypt | Used for secret notary |
| `rustls` | 0.23.36 | TLS implementation | Used by server |
| `tonic` | 0.14.3 | gRPC framework | `tls-ring` feature |
| `sha256` | 1.6.0 | Content hashing | Used for digest computation |
| `base64` | 0.22.1 | Encoding | Used in notary |
| `aws-config` / `aws-sdk-s3` | 1.8.1 / 1.124.0 | S3 backend | Standard AWS credential chain |

The project uses `ring` as the default crypto provider (`ring::default_provider().install_default()`), installed at CLI startup.

## 10. .gitignore Security

The root `.gitignore` excludes `.env` and `.env.*` files, preventing accidental commit of environment-based secrets. The Terraform `.gitignore` excludes `*.tfvars`, `*.tfstate`, and `.terraform/`, preventing state and variable files from being committed.

## 11. Known Gaps and Risks

### Critical

1. **No sandbox isolation:** Build steps execute as the server user without filesystem, network, or resource isolation. This is by design for the current architecture but represents a significant trust boundary — artifact definitions must be trusted.

2. **Archive push lacks authorization:** When OIDC auth is enabled, the archive `push` endpoint does not check namespace permissions, allowing any authenticated user to push archives to any namespace.

### High

3. **PKCS1v15 padding for secrets:** The notary module uses PKCS1v15 padding, which is vulnerable to padding oracle attacks in certain contexts. OAEP is the recommended RSA encryption padding scheme.

4. **CLI arguments expose secrets:** The `--issuer-client-secret` flag is visible in process listings (`ps`). Consider reading from a file or environment variable.

5. **No key file permissions:** Generated private keys are written with the default umask, potentially readable by other users.

### Medium

6. **No certificate rotation:** The PKI has no expiration or rotation mechanism.

7. **Namespace auth not wired:** The `namespaces` claim-based authorization is implemented in code but the corresponding Keycloak configuration does not provision this claim. The system will effectively deny all namespace-scoped operations for authenticated users unless manually configured.

8. **Credentials stored in plaintext:** Access and refresh tokens in `credentials.json` are not encrypted at rest.

9. **Unconstrained CA:** The CA certificate allows signing any certificate, not limited to Vorpal service certificates.

10. **Health check always plaintext:** The health check listener runs without TLS even when the main listener uses TLS.
