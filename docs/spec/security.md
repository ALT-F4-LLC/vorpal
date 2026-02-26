# Security Specification

This document describes the security model, authentication/authorization boundaries, secret
management, and trust boundaries as they actually exist in the Vorpal codebase today.

---

## 1. Authentication

### 1.1 OIDC / JWT Authentication (Server-Side)

Vorpal's gRPC services support optional OIDC-based JWT authentication. When the `--issuer` flag
is provided to `vorpal system services start`, an `OidcValidator` is instantiated and attached as
a tonic interceptor on the registry (archive + artifact) and worker services.

**Implementation:** `cli/src/command/start/auth.rs`

- **OIDC Discovery:** On startup, the validator fetches `/.well-known/openid-configuration` from
  the configured issuer URL and extracts the `jwks_uri` and `issuer` fields.
- **Issuer Verification:** The discovered issuer is compared (after trailing-slash normalization)
  against the configured issuer. Mismatch is a hard error at startup.
- **JWKS Fetching and Caching:** The JSON Web Key Set is fetched at startup and cached in-memory
  behind an `Arc<RwLock<JwkSet>>`. If a token presents a `kid` not found in the cache, the JWKS
  is re-fetched once (handles key rotation).
- **Token Validation:** Performed via the `jsonwebtoken` crate with:
  - Algorithm: RS256 only (RSA key type enforced)
  - Audience validation enabled (`validate_aud = true`)
  - Expiration validation enabled (`validate_exp = true`)
  - Not-before validation enabled (`validate_nbf = true`)
  - Issuer validation with trailing-slash tolerance (accepts both `issuer` and `issuer/`)
- **Claims Extraction:** Validated claims (`Claims` struct) are inserted into `request.extensions()`
  for downstream handlers to consume. Includes `sub`, `aud`, `exp`, `iss`, `scope`, `azp`, `gty`,
  and a custom `namespaces` field for authorization.

**Authentication is optional.** When `--issuer` is not provided, services run without any
authentication interceptor. All gRPC endpoints are then open to any caller.

### 1.2 OAuth2 Device Authorization Grant (CLI Login)

The `vorpal login` command implements the OAuth2 Device Authorization Grant flow
(RFC 8628) for end-user authentication.

**Implementation:** `cli/src/command.rs` (Login command handler)

- Uses the `oauth2` crate's `BasicClient` with device code exchange.
- Requests `offline_access` scope to obtain a refresh token.
- Displays a verification URL and user code; polls for token completion.
- On success, stores credentials to `/var/lib/vorpal/key/credentials.json`.

### 1.3 OAuth2 Client Credentials Flow (Service-to-Service)

Workers authenticate to the registry using the OAuth2 Client Credentials Grant.

**Implementation:** `cli/src/command/start/auth.rs` (`exchange_client_credentials`)

- Discovers the token endpoint via OIDC discovery.
- Exchanges `client_id` and `client_secret` for an access token using form-encoded POST.
- Returns the bearer token as a tonic `MetadataValue` for gRPC calls.
- Client credentials are passed via `--issuer-client-id` and `--issuer-client-secret` CLI flags.

### 1.4 Token Refresh (Client-Side)

The SDK's `client_auth_header` function (`sdk/rust/src/context.rs`) handles automatic token
refresh:

- Reads credentials from `/var/lib/vorpal/key/credentials.json`.
- Checks if the token has less than 5 minutes remaining (`token_age + 300 >= expires_in`).
- If expired and a refresh token exists, performs an OIDC refresh token exchange.
- Writes the refreshed credentials back to disk.
- If no refresh token is available, returns an error directing the user to re-login.

---

## 2. Authorization

### 2.1 Namespace-Based Permissions

Authorization is enforced per-namespace with permission checks at the gRPC handler level.

**Implementation:** `cli/src/command/start/auth.rs` (`require_namespace_permission`)

- Extracts `Claims` from `request.extensions()`.
- Checks the `namespaces` claim (a `HashMap<String, Vec<String>>`) for:
  - Exact namespace match with the required permission string.
  - Wildcard admin access via the `*` namespace key.
- Returns `Status::permission_denied` (gRPC code 7 / HTTP 403) on failure.
- Returns `Status::unauthenticated` (gRPC code 16 / HTTP 401) if no claims are found.

### 2.2 Permission Checks by Service

| Service | Endpoint | Permission Required |
|---------|----------|-------------------|
| Archive | `pull` | `read` on archive namespace |
| Archive | `check` | None (no auth check on `check`) |
| Archive | `push` | None (no auth check on `push`) |
| Artifact | `get_artifact` | `read` on artifact namespace |
| Artifact | `get_artifact_alias` | `read` on artifact namespace |
| Artifact | `store_artifact` | `write` on artifact namespace |
| Worker | `build_artifact` | `write` on artifact namespace |

**Notable gap:** The archive `check` and `push` endpoints skip authorization checks even when
authentication is enabled. The `check` endpoint is called with `request.into_inner()` before
any auth check. The `push` endpoint processes the streaming request without any namespace
permission verification. This means any authenticated user can push archives to any namespace
and check archive existence.

### 2.3 Conditional Authorization

Authorization checks are conditional -- they only execute when `Claims` are present in the
request extensions:

```rust
if request.extensions().get::<Claims>().is_some() {
    require_namespace_permission(&request, &namespace, "read")?;
}
```

This means when auth is disabled (no `--issuer`), no authorization is enforced. This is
by design for local development.

### 2.4 Audit Logging

`get_user_context()` extracts the `sub` (subject) claim for audit logging in gRPC handlers.
User identity is logged via `tracing::info!` when operations are performed on archive and
artifact services. This is informational only -- there is no structured audit log sink.

---

## 3. Keycloak Integration

### 3.1 Keycloak as Identity Provider

Vorpal uses Keycloak as its reference OIDC identity provider, configured via Terraform.

**Implementation:** `terraform/module/keycloak/`

- **Realm:** `vorpal`
- **Clients:**
  - `cli` -- PUBLIC access type, device authorization grant enabled, optional scopes for
    archive/artifact/worker
  - `archive` -- CONFIDENTIAL, token exchange enabled, roles: `archive:check`, `archive:push`,
    `archive:pull`
  - `artifact` -- CONFIDENTIAL, token exchange enabled, roles: `artifact:get`,
    `artifact:get-alias`, `artifact:store`
  - `worker` -- CONFIDENTIAL, service accounts enabled, token exchange enabled, optional scopes
    for archive and artifact, roles: `worker:build-artifact`

### 3.2 Development Keycloak

A docker-compose file provisions a development Keycloak instance:

**File:** `docker-compose.yaml`

- Image: `quay.io/keycloak/keycloak:26.5.2`
- Runs in `start-dev` mode (development, NOT production)
- **Hardcoded credentials:** `admin` / `password`
- Bound to `127.0.0.1:8080` (localhost only)

### 3.3 Test Users

**File:** `terraform/module/keycloak/local.tf`

- A single test user `admin` with email `admin@localhost` and **hardcoded password `password`**.
- These are intended for local development only and should never be used in production.

### 3.4 Keycloak Test Script

**File:** `script/test/keycloak.sh`

- Exercises the full device authorization flow, token exchange, and introspection.
- Requires `ARCHIVE_CLIENT_SECRET`, `ARTIFACT_CLIENT_SECRET`, and `WORKER_CLIENT_SECRET`
  environment variables.
- Client secrets are never hardcoded in the script.

---

## 4. Transport Layer Security (TLS)

### 4.1 Server TLS

TLS for the main gRPC listener is optional, enabled via `--tls` flag.

**Implementation:** `cli/src/command/start.rs` (`new_tls_config`)

- Reads the service certificate from `/var/lib/vorpal/key/service.pem`.
- Reads the private key from `/var/lib/vorpal/key/service.key.pem`.
- Constructs a `ServerTlsConfig` with `Identity::from_pem`.
- When TLS is enabled without an explicit `--port`, defaults to TCP port `23151`.
- When TLS is disabled, the server can operate over plaintext TCP or Unix domain sockets.

### 4.2 Client TLS

**Implementation:** `sdk/rust/src/context.rs` (`get_client_tls_config`)

- For `http://` and `unix://` URIs: no TLS.
- For `https://` URIs: if `/var/lib/vorpal/key/ca.pem` exists, it is used as the CA certificate.
  Otherwise, the system's native root certificates are used via `with_native_roots()`.

### 4.3 Health Check Endpoint

The health check listener (`--health-check`) always runs over plaintext TCP on a separate port
(default `23152`). This is by design for load balancer/orchestrator probes. The port must differ
from the main service port (enforced at startup).

---

## 5. Key Management

### 5.1 PKI Infrastructure

**Implementation:** `cli/src/command/system/keys.rs`

The `vorpal system keys generate` command creates a self-signed PKI hierarchy:

| Key | Path | Purpose |
|-----|------|---------|
| CA private key | `/var/lib/vorpal/key/ca.key.pem` | Signs service certificates |
| CA certificate | `/var/lib/vorpal/key/ca.pem` | Trust anchor for TLS |
| Service private key | `/var/lib/vorpal/key/service.key.pem` | TLS server identity + secret decryption |
| Service public key | `/var/lib/vorpal/key/service.public.pem` | Secret encryption |
| Service certificate | `/var/lib/vorpal/key/service.pem` | TLS server certificate (signed by CA) |
| Service secret | `/var/lib/vorpal/key/service.secret` | UUIDv7, purpose unclear |

- Algorithm: RSA with PKCS#8/SHA-256 (`PKCS_RSA_SHA256` via `rcgen`).
- CA is self-signed with `IsCa::Ca(BasicConstraints::Unconstrained)`.
- Service certificate is for `localhost` only, with `ServerAuth` extended key usage.
- Key generation is idempotent: each key is only generated if the file does not already exist.
- No key rotation mechanism exists.

### 5.2 Secret Encryption (Notary)

**Implementation:** `cli/src/command/store/notary.rs`

Artifact step secrets are encrypted at rest and in transit:

- **Encryption:** RSA PKCS#1 v1.5 (`Pkcs1v15Encrypt`) with the service public key.
  Ciphertext is base64-encoded.
- **Decryption:** RSA PKCS#1 v1.5 with the service private key. Base64-decoded then decrypted.
- Encryption happens on the agent when preparing artifacts.
- Decryption happens on the worker when executing build steps.
- Secrets are injected as environment variables during step execution.

**Security note:** PKCS#1 v1.5 padding is known to be vulnerable to Bleichenbacher-style
padding oracle attacks. OAEP padding would be a more secure choice, though exploitation
requires an oracle which may not be present in this architecture.

### 5.3 Credentials Storage

**Path:** `/var/lib/vorpal/key/credentials.json`

Stores OAuth2 tokens in the `VorpalCredentials` struct:

```json
{
  "issuer": {
    "<issuer-url>": {
      "access_token": "...",
      "audience": "...",
      "client_id": "...",
      "expires_in": 300,
      "issued_at": 1700000000,
      "refresh_token": "...",
      "scopes": ["offline_access"]
    }
  },
  "registry": {
    "<registry-url>": "<issuer-url>"
  }
}
```

- Stored as plaintext JSON on disk.
- No file permission restrictions are enforced at write time (inherits umask).
- Access tokens and refresh tokens are stored in cleartext.
- The file is read and written by the CLI on every authenticated operation.

---

## 6. Trust Boundaries

### 6.1 Architecture Trust Model

```
CLI (client) --[gRPC/TLS]--> Agent --[gRPC]--> Registry
                                   |                |
                                   |                v
                              Worker --[gRPC]--> Registry (archive + artifact)
```

**Trust boundaries:**

1. **CLI to Agent/Registry:** The first trust boundary. When TLS is enabled, the connection
   is encrypted. Authentication is via bearer token in gRPC metadata. When using Unix domain
   sockets, access control relies on filesystem permissions (socket set to `0o660`).

2. **Agent to Registry:** The agent uses `client_auth_header()` to attach tokens when connecting
   to remote registries. For local (UDS) connections, no auth header is attached.

3. **Worker to Registry:** Workers obtain their own service tokens via client credentials flow,
   independent of the user token. This is a service-to-service trust boundary.

4. **Registry to Storage Backend:** The registry delegates storage to either the local filesystem
   or S3. S3 authentication uses the standard AWS SDK credential chain (environment variables,
   instance profile, etc.). No additional Vorpal-level authentication exists for the storage
   backend.

### 6.2 Process Execution Trust Boundary

The worker executes arbitrary commands as build steps:

**Implementation:** `cli/src/command/start/worker.rs` (`run_step`)

- Steps execute via `tokio::process::Command` in the worker's process context.
- No sandboxing, containerization, or privilege separation exists.
- Steps run with the full permissions of the worker process.
- Environment variables (including decrypted secrets) are passed directly to the child process.
- The `entrypoint` field allows specifying any executable path.
- Scripts are written to a temporary workspace directory with `0o755` permissions.

This is the most significant trust boundary: anyone with `write` permission on a namespace
can execute arbitrary code on the worker.

### 6.3 Unix Domain Socket Boundary

When operating in UDS mode (default, no `--port`):

- Socket path: `/var/lib/vorpal/vorpal.sock` (or `VORPAL_SOCKET_PATH` env var).
- Permissions: `0o660` (owner + group read/write).
- An advisory file lock (`/var/lib/vorpal/vorpal.lock`) prevents multiple instances.
- Stale socket detection via connection attempt before removal.
- Permission-denied errors on existing sockets are treated as fatal (another user's socket).

---

## 7. Data Integrity

### 7.1 Content Addressability

All artifacts and archives are identified by their SHA-256 digest:

- **Source digests:** Computed from file contents using `sha256::try_digest` per file,
  then combined into a single digest (`get_source_digest` in `cli/src/command/store/hashes.rs`).
- **Artifact digests:** SHA-256 of the JSON-serialized `Artifact` protobuf message.
- **Digest verification:** Sources with a pre-existing digest in the lockfile are verified
  against the computed digest. Mismatch is a hard error unless `--unlock` is specified.

### 7.2 Lockfile Integrity

**File:** `Vorpal.lock`

- Records source name, path, digest, platform, includes, and excludes.
- Changes to locked sources require explicit `--unlock` flag.
- Lockfile is updated after successful source preparation for HTTP sources.
- No cryptographic signing of the lockfile itself.

### 7.3 Archive Integrity

- Archives use zstd-compressed tar format (`.tar.zst`).
- File timestamps are normalized to Unix epoch (0) for reproducibility.
- `.git` directories are always excluded from source collections.
- No integrity verification is performed after S3 upload/download beyond what the AWS SDK
  provides (S3 checksums).

---

## 8. Supply Chain Security

### 8.1 Dependency Management

- **Rust dependencies:** Managed via `Cargo.lock` with exact version pinning.
- **Renovate:** Configured via `.github/renovate.json` with weekly lock file maintenance.
  Template directories are excluded from Renovate scanning.
- **Vendoring:** `make vendor` copies all dependencies to a local `vendor/` directory for
  offline builds.

### 8.2 CI/CD Security

**File:** `.github/workflows/vorpal.yaml`

- **Secrets used:**
  - `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` for S3 registry backend
  - `DOCKERHUB_TOKEN` / `DOCKERHUB_USERNAME` for container image publishing
  - `ALTF4LLC_GITHUB_APP_ID` / `ALTF4LLC_GITHUB_APP_PRIVATE_KEY` for nightly releases
- **Permissions:** Release jobs request `attestations: write`, `contents: write`, `id-token: write`,
  and `packages: write` -- scoped to tagged pushes only.
- **Build provenance:** `actions/attest-build-provenance@v3` generates SLSA provenance
  attestations for release binaries.
- **Concurrency:** CI runs are cancelled for superseded commits within the same PR/branch.

### 8.3 HTTP Source Downloads

The agent downloads sources from arbitrary HTTP/HTTPS URLs (`cli/src/command/start/agent.rs`):

- URL scheme is validated (must be `http` or `https`).
- Response status is checked (`is_success()`).
- Content type is inferred from bytes (`infer::get`), not from HTTP headers.
- Archives (gzip, bzip2, xz, zip) are automatically unpacked.
- **No certificate pinning** or custom TLS configuration for HTTP downloads -- uses the
  `reqwest` defaults with `rustls-tls`.

---

## 9. Environment Variables and Configuration

### 9.1 Security-Relevant Environment Variables

| Variable | Purpose | Used By |
|----------|---------|---------|
| `VORPAL_SOCKET_PATH` | Override Unix socket path | CLI, SDK |
| `AWS_ACCESS_KEY_ID` | S3 storage authentication | Registry (via AWS SDK) |
| `AWS_SECRET_ACCESS_KEY` | S3 storage authentication | Registry (via AWS SDK) |
| `AWS_DEFAULT_REGION` | S3 region configuration | Registry (via AWS SDK) |

### 9.2 Sensitive CLI Flags

| Flag | Service | Sensitivity |
|------|---------|-------------|
| `--issuer-client-secret` | `system services start` | OAuth2 client secret for worker service account |
| `--issuer-client-id` | `system services start` | OAuth2 client ID |
| `--issuer` | `system services start` / `login` | OIDC issuer URL |

These are passed as command-line arguments, which means they may be visible in process listings.

### 9.3 .gitignore Coverage

The root `.gitignore` excludes:
- `.env` and `.env.*` (environment files)
- `.cargo` (local cargo config)
- `target` (build artifacts)
- `vendor` (vendored dependencies)
- `.docket` (task management)
- `dist` (distribution artifacts)

The terraform `.gitignore` excludes `.tfvars`, `.tfstate`, and `.terraform`.

---

## 10. Known Gaps and Risks

### 10.1 High Priority

1. **No sandbox for build steps.** Workers execute arbitrary commands with the full privileges
   of the worker process. Any user with namespace write access can run arbitrary code on the
   worker host. There is no containerization, chroot, seccomp, or capability dropping.

2. **Archive push has no authorization check.** The `ArchiveService::push` endpoint does not
   verify namespace permissions, allowing any authenticated user to push archive data to any
   namespace.

3. **Archive check has no authorization check.** The `ArchiveService::check` endpoint does not
   verify namespace permissions, allowing any authenticated user to check existence of archives
   in any namespace.

4. **PKCS#1 v1.5 padding for secret encryption.** The notary module uses RSA PKCS#1 v1.5
   encryption, which is considered legacy. RSA-OAEP is the recommended replacement.

5. **Client secrets in CLI arguments.** The `--issuer-client-secret` flag exposes the secret in
   process listings (`/proc/<pid>/cmdline` on Linux).

### 10.2 Medium Priority

6. **No key rotation.** Once generated, keys are never rotated. No mechanism exists to rotate
   the CA, service keys, or service secret.

7. **Credentials stored in cleartext.** Access tokens and refresh tokens in
   `/var/lib/vorpal/key/credentials.json` are not encrypted at rest and have no enforced file
   permissions beyond umask.

8. **Service certificate hardcoded to localhost.** The generated TLS certificate is only valid
   for `localhost`, making it unsuitable for production deployments without manual certificate
   management.

9. **No rate limiting.** gRPC endpoints have no rate limiting or request throttling. A
   malicious or misbehaving client could overwhelm the server.

10. **Development Keycloak credentials.** The docker-compose and Terraform configs contain
    hardcoded `admin`/`password` credentials. These are clearly for development only, but could
    be accidentally deployed.

### 10.3 Low Priority

11. **JWKS cache has no TTL.** The JWKS cache is only refreshed when a `kid` is not found.
    There is no periodic refresh, meaning revoked keys may remain cached indefinitely.

12. **No structured audit log.** Audit information (user identity, action, namespace) is logged
    via `tracing::info!` but there is no dedicated audit log format, storage, or forwarding.

13. **Commented-out validation code.** `auth.rs` contains commented-out `validate_claims` method
    and several `AuthError` variants. While the `jsonwebtoken` library handles these checks via
    `Validation` config, the dead code may cause confusion during review.

14. **Login command overwrites credentials.** The `vorpal login` command creates a new
    credentials file each time rather than merging with existing credentials (noted with
    `// TODO: load existing credentials file if it exists`).
