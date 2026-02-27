# Security Specification

> Documents the authentication, authorization, secret management, transport security, and trust
> boundaries that exist in the Vorpal codebase today. Honest about gaps.

---

## 1. Authentication

### 1.1 OIDC / OAuth2 (Server-Side)

The registry and worker gRPC services optionally enforce OIDC JWT authentication via a
`tonic` interceptor defined in `cli/src/command/start/auth.rs`.

| Aspect | Implementation |
|---|---|
| **Protocol** | OpenID Connect Discovery + RS256 JWT validation |
| **Provider** | Keycloak (bundled via `docker-compose.yaml` and `terraform/module/keycloak/`) |
| **Discovery** | `{issuer}/.well-known/openid-configuration` fetched at startup |
| **JWKS** | Fetched from discovered `jwks_uri`; cached in `Arc<RwLock<JwkSet>>` with lazy refresh on `kid` miss |
| **Validation** | `jsonwebtoken` crate: RS256, audience check, issuer check (with/without trailing `/`), `exp` + `nbf` validation |
| **Claims** | Custom `Claims` struct: `sub`, `aud`, `iss`, `exp`, `scope`, `azp`, `gty`, `namespaces` |
| **Interceptor** | Sync gRPC interceptor using `tokio::task::block_in_place` to bridge async validation |
| **Opt-in** | Auth is only enabled when `--issuer` is passed to `vorpal system services start`. Without it, **all services run unauthenticated** |

### 1.2 OAuth2 Client Credentials Flow (Service-to-Service)

The worker service obtains its own tokens to call the registry's archive and artifact services.

- Implemented in `auth::exchange_client_credentials()` (`cli/src/command/start/auth.rs:314`)
- Uses `client_id` + `client_secret` via OIDC `token_endpoint`
- Scopes requested: `read:archive write:archive`, `read:artifact write:artifact`
- Credentials are passed via CLI flags: `--issuer-client-id`, `--issuer-client-secret`

### 1.3 OAuth2 Device Authorization Flow (CLI Login)

End-users authenticate via `vorpal login`:

- Uses OAuth2 Device Authorization Grant (`exchange_device_code`)
- Provider: configurable via `--issuer` (default: `http://localhost:8080/realms/vorpal`)
- Requests `offline_access` scope for refresh token support
- Tokens stored in `/var/lib/vorpal/key/credentials.json`

### 1.4 Token Refresh

Both the Rust SDK (`sdk/rust/src/context.rs`) and Go SDK (`sdk/go/pkg/config/context.go`)
implement automatic token refresh:

- Tokens refreshed when `token_age + 300 >= expires_in` (5-minute buffer before expiry)
- Refresh uses `grant_type=refresh_token` via OIDC `token_endpoint`
- Updated credentials written back to `credentials.json` on disk
- If no refresh token exists, the user is prompted to re-login

---

## 2. Authorization

### 2.1 Namespace-Based Permissions

Authorization is namespace-scoped using JWT claims:

```rust
// Claims struct includes:
pub namespaces: Option<HashMap<String, Vec<String>>>
```

- `require_namespace_permission(request, namespace, permission)` checks claims
- Permissions are strings like `"read"` or `"write"` mapped to namespaces
- Wildcard `"*"` namespace grants admin access to all namespaces
- Applied to: `archive pull/push`, `artifact get/store/get_alias`, `worker build_artifact`
- Returns gRPC `PERMISSION_DENIED` (403) on failure

### 2.2 Authorization is Optional

When `--issuer` is not configured:

- No interceptor is attached to gRPC services
- All registry endpoints (archive, artifact) are fully open
- Worker builds are unrestricted
- The auth check in handlers uses `if request.extensions().get::<Claims>().is_some()` — if no
  claims exist (no interceptor), the authorization block is entirely skipped

### 2.3 Audit Logging

- `get_user_context()` extracts the `sub` claim for audit logging
- Logged on: archive pull, artifact get, artifact store, worker build requests
- Uses `tracing::info!` — no structured audit log system

---

## 3. Transport Security

### 3.1 TLS (Server)

TLS for the gRPC server is opt-in via `--tls` flag:

- Certificate generation: `vorpal system keys generate` (`cli/src/command/system/keys.rs`)
- Uses `rcgen` crate with `PKCS_RSA_SHA256` algorithm
- Generates: CA keypair + self-signed cert, service keypair + CA-signed cert (SAN: `localhost`)
- Key storage: `/var/lib/vorpal/key/`
  - `ca.key.pem` — CA private key
  - `ca.pem` — CA certificate
  - `service.key.pem` — Service private key
  - `service.pem` — Service certificate (CA-signed)
  - `service.public.pem` — Service public key
  - `service.secret` — UUID v7 service secret
- TLS config uses `tonic::transport::ServerTlsConfig` with `Identity::from_pem`
- Health check listener is always plaintext (separate port), even when TLS is enabled

### 3.2 TLS (Client)

Both Rust and Go SDKs auto-detect transport based on URI scheme:

| Scheme | Transport |
|---|---|
| `http://` | Plaintext (insecure) |
| `https://` | TLS — loads CA cert from `/var/lib/vorpal/key/ca.pem` if available, else uses system trust store |
| `unix://` | Unix domain socket (plaintext, no TLS) |

### 3.3 Default Transport

- Local development defaults to Unix domain socket (`/var/lib/vorpal/vorpal.sock`)
- Socket permissions set to `0o660` (owner + group read/write)
- UDS path overridable via `VORPAL_SOCKET_PATH` environment variable
- TCP mode activated by `--port` or `--tls` flags

---

## 4. Secret Management

### 4.1 Build-Time Secrets

Artifact build steps support encrypted secrets via the `ArtifactStepSecret` protobuf message:

```protobuf
message ArtifactStepSecret {
    string name = 1;
    string value = 2;
}
```

- Secrets are RSA-encrypted (PKCS1v15) by the client using the service's public key
- Decryption happens at build time on the worker (`cli/src/command/start/worker.rs:480`)
- Decrypted secrets are injected as environment variables into the build step
- Encryption/decryption uses `rsa` crate with `rand_core::OsRng`
- Base64 encoding for transport (`base64::engine::general_purpose::STANDARD`)
- Implementation in `cli/src/command/store/notary.rs`

### 4.2 Credential Storage

| Path | Content | Notes |
|---|---|---|
| `/var/lib/vorpal/key/credentials.json` | OIDC tokens (access, refresh), client IDs, issuer mappings | Written by `vorpal login`; read by SDK auth header functions |
| `/var/lib/vorpal/key/ca.key.pem` | CA private key | Generated by `vorpal system keys generate` |
| `/var/lib/vorpal/key/service.key.pem` | Service private key | Used for TLS and secret decryption |
| `/var/lib/vorpal/key/service.secret` | UUID v7 service secret | Purpose unclear — generated but not obviously consumed |

### 4.3 AWS Credentials

The S3 registry backend uses `aws-config` with `BehaviorVersion::latest()`:

- AWS credentials are resolved through the standard AWS SDK credential chain (env vars, IAM roles, profiles)
- No explicit credential handling in the codebase — fully delegated to `aws-config`
- CI uses `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` GitHub secrets

### 4.4 Docker Hub Credentials

CI pushes container images using Docker Hub credentials:

- `DOCKERHUB_TOKEN` and `DOCKERHUB_USERNAME` stored as GitHub Actions secrets
- Used via `docker/login-action@v3`

---

## 5. Trust Boundaries

### 5.1 Boundary: CLI -> Agent/Registry/Worker

- CLI connects to agent, registry, and worker services
- Authentication is optional (see Section 2.2)
- When auth is configured, Bearer tokens are attached per-request via `client_auth_header()`
- Token refresh is automatic in both Rust and Go SDKs

### 5.2 Boundary: Worker -> Registry

- Worker uses OAuth2 Client Credentials flow to obtain its own token
- Separate tokens obtained for archive (`read:archive write:archive`) and artifact (`read:artifact write:artifact`) scopes
- If credentials are not configured, worker operates without auth headers

### 5.3 Boundary: Registry -> Storage Backend

- **Local backend**: Direct filesystem access under `/var/lib/vorpal/store/`
- **S3 backend**: AWS SDK handles auth; no application-level credential management

### 5.4 Boundary: Build Sandbox

Linux builds use `bubblewrap` (`bwrap`) for sandboxing:

- `--unshare-all` + `--share-net`: Unshares all namespaces except network
- Runs as UID/GID 1000 (non-root)
- Filesystem: bind-mounts for rootfs (`--ro-bind`), output dir, and workspace dir
- Artifacts mounted read-only (`--ro-bind`)
- Private `/dev`, `/proc`, `/tmp`
- macOS builds use direct `bash` execution (no sandboxing)

---

## 6. Cryptographic Primitives

| Purpose | Algorithm/Library | Notes |
|---|---|---|
| JWT validation | RS256 via `jsonwebtoken` crate | Server-side OIDC token verification |
| TLS certificates | RSA + SHA256 via `rcgen` (`PKCS_RSA_SHA256`) | Self-signed CA, CA-signed service cert |
| Secret encryption | RSA PKCS1v15 via `rsa` crate | Build-time secret protection |
| Artifact integrity | SHA-256 via `sha256` crate (Rust), `crypto/sha256` (Go) | Content-addressable artifact digests |
| TLS runtime | `ring` via `rustls` | `ring::default_provider()` installed at CLI startup |
| HTTP TLS | `rustls-tls` feature on `reqwest` | All HTTP clients use rustls, not OpenSSL |

---

## 7. Input Validation

### 7.1 Artifact Alias Parsing

Both Rust and Go SDKs validate alias components (`parse_artifact_alias()`):

- Max length: 255 characters
- Allowed chars: `[a-zA-Z0-9\-._+]`
- No empty components, no multiple path separators
- Consistent validation across both SDKs

### 7.2 Binary Name Validation (Run Command)

`cli/src/command/run.rs` validates binary names before execution:

- No path separators (`/`, `\`)
- No leading dots
- Non-empty
- Executable permission check (`mode & 0o111`)

### 7.3 S3 Key Validation (Artifact Storage)

`cli/src/command/start/registry/artifact/s3.rs` validates alias names before S3 storage:

- No `/`, `\`, null bytes, whitespace
- No leading/trailing `.` or `-`
- Max 255 characters
- Alphanumeric, `_`, `-`, `.` only

---

## 8. Environment Variables

| Variable | Purpose | Where Used |
|---|---|---|
| `VORPAL_SOCKET_PATH` | Override default Unix socket path | `cli/src/command/store/paths.rs` |
| `AWS_ACCESS_KEY_ID` | AWS S3 credentials | CI + AWS SDK chain |
| `AWS_SECRET_ACCESS_KEY` | AWS S3 credentials | CI + AWS SDK chain |
| `AWS_DEFAULT_REGION` | AWS region | CI + AWS SDK chain |
| `VORPAL_OUTPUT` | Build output directory (injected into steps) | Worker build step execution |
| `VORPAL_WORKSPACE` | Build workspace directory (injected into steps) | Worker build step execution |
| `VORPAL_ARTIFACT_*` | Artifact dependency paths (injected into steps) | Worker build step execution |

---

## 9. CI/CD Security

### 9.1 GitHub Actions Workflow (`vorpal.yaml`)

- Uses pinned major versions of actions (`@v6`, `@v7`, `@v3`, `@v2`)
- Build provenance attestation via `actions/attest-build-provenance@v3` on releases
- Secrets: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `DOCKERHUB_TOKEN`, `DOCKERHUB_USERNAME`
- Release permissions: `attestations: write`, `contents: write`, `id-token: write`, `packages: write`
- Renovate configured for dependency updates (`.github/renovate.json`)

### 9.2 Supply Chain

- Rust dependencies vendored (`cargo vendor`) for offline/reproducible builds
- `.gitignore` excludes `.env`, `.env.*` files
- Cargo.lock committed for deterministic builds

---

## 10. Keycloak Configuration (Terraform)

The `terraform/module/keycloak/` module defines the authorization model:

| Client | Type | Capabilities |
|---|---|---|
| `cli` | PUBLIC | Device auth grant, optional scopes: archive, artifact, worker |
| `archive` | CONFIDENTIAL | Token exchange, roles: `archive:check`, `archive:push`, `archive:pull` |
| `artifact` | CONFIDENTIAL | Token exchange, roles: `artifact:get`, `artifact:get-alias`, `artifact:store` |
| `worker` | CONFIDENTIAL | Token exchange, service accounts, optional scopes: archive, artifact, role: `worker:build-artifact` |

Dev environment includes a default admin user with password `password` (appropriate for local dev only).

---

## 11. Known Gaps and Risks

### 11.1 Authentication is Fully Optional

The entire auth system is opt-in. Running `vorpal system services start` without `--issuer`
means zero authentication and zero authorization on all registry and worker endpoints. There is
no warning or prompt when running without auth in non-local (TCP) mode.

### 11.2 No Rate Limiting

No rate limiting exists on any gRPC endpoint. An unauthenticated registry (or one with valid
credentials) can be abused for unbounded archive pushes.

### 11.3 No mTLS

Only server-side TLS is implemented. Clients authenticate via Bearer tokens, not client
certificates. The worker authenticates to the registry via OAuth2 tokens, not mTLS.

### 11.4 Credential File Permissions

The credentials file at `/var/lib/vorpal/key/credentials.json` does not enforce strict file
permissions. The Go SDK writes with `0o600`, but the Rust SDK uses the default umask. Private
keys are similarly written without explicit restrictive permissions.

### 11.5 No Token Revocation

There is no token revocation mechanism. If a token or refresh token is compromised, it remains
valid until expiry. No token blacklist or revocation endpoint is integrated.

### 11.6 Sync Interceptor Workaround

The OIDC validator runs async JWKS fetching inside a `block_in_place` call within a sync gRPC
interceptor. This works but could cause thread-pool starvation under high concurrency. The code
acknowledges this with a comment suggesting a tower layer for high-throughput scenarios.

### 11.7 No macOS Build Sandboxing

macOS builds execute directly via `bash` without any sandboxing (no `bwrap` equivalent). Build
steps have full access to the host filesystem, network, and environment.

### 11.8 Service Secret Purpose Unclear

`/var/lib/vorpal/key/service.secret` is generated (UUID v7) by `vorpal system keys generate`
but its consumption path is not obvious in the codebase.

### 11.9 Docker Compose Keycloak Uses Dev Mode

`docker-compose.yaml` runs Keycloak with `start-dev` and hardcoded admin credentials
(`admin`/`password`). This is appropriate for local development but must not reach production.

### 11.10 Archive Push Has No Auth Check

The `ArchiveService::push` implementation in `cli/src/command/start/registry.rs` does not
include a namespace permission check. Archive `pull` checks `read` permission, but `push` does
not call `require_namespace_permission` for `write`. The authorization check must be verified —
it may be handled by the interceptor at the transport level, but the handler-level check that
exists on `pull`, `get_artifact`, and `store_artifact` is missing on `push`.

### 11.11 Login Overwrites Credentials

`vorpal login` creates a fresh `VorpalCredentials` object and overwrites the credentials file.
The code contains a `TODO: load existing credentials file if it exists`. This means logging into
a second registry will erase credentials for the first.
