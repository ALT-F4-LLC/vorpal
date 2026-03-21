---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Security posture of the Vorpal build system — authentication, authorization, cryptography, secret management, supply chain, and trust boundaries"
owner: "@staff-engineer"
dependencies:
  - architecture.md
---

# Security Specification

## 1. Overview

Vorpal is a build system with a client-server architecture composed of four gRPC services (agent, registry/archive, registry/artifact, worker) plus a CLI client. Security spans several domains: transport encryption, identity and access control, secret management, build integrity, and supply chain. Authentication and authorization are **optional** — when not configured, all services run unauthenticated. This is by design for local development but is a critical deployment consideration for shared or production registries.

## 2. Transport Security

### 2.1 TLS for gRPC (Server-Side)

TLS is opt-in via the `--tls` flag on `vorpal system services start`. When enabled:

- Server identity is loaded from locally-generated PEM certificates at `/var/lib/vorpal/key/service.pem` (cert) and `/var/lib/vorpal/key/service.key.pem` (private key).
- TLS configuration uses `tonic::transport::ServerTlsConfig` with a single identity (no mTLS).
- TLS implies TCP transport on port 23151 (default). Without `--tls`, the default transport is a Unix domain socket at `/var/lib/vorpal/vorpal.sock`.

**Implementation:** `cli/src/command/start.rs:51-85` (`new_tls_config`).

### 2.2 TLS for gRPC (Client-Side)

Client TLS behavior in `sdk/rust/src/context.rs:573-591`:

- `http://` and `unix://` URIs: no TLS.
- `https://` URIs: TLS enabled. If `/var/lib/vorpal/key/ca.pem` exists, it is used as the trusted CA (for self-signed infrastructure). Otherwise, system native roots are used (`ClientTlsConfig::new().with_native_roots()`).

### 2.3 Unix Domain Socket

Default local transport. Socket created at `/var/lib/vorpal/vorpal.sock` (overridable via `VORPAL_SOCKET_PATH` env var). Socket permissions are set to `0o660` (`cli/src/command/start.rs:423`), restricting access to the owner and group.

### 2.4 Health Check Endpoint

The health check listener (`--health-check`) is **always plaintext TCP**, even when the main listener uses TLS. This is intentional for load balancer probing but means the health endpoint has no transport security.

### 2.5 Gaps

- **No mTLS:** Server does not verify client certificates. Any client that trusts the server CA (or skips verification) can connect.
- **No TLS on health endpoint:** Health responses are unencrypted.

## 3. Authentication

### 3.1 OIDC Token Validation (Server-Side)

When `--issuer` is provided to `vorpal system services start`, the registry (archive + artifact) and worker services are wrapped with an OIDC interceptor that validates JWT bearer tokens on every request.

**Implementation:** `cli/src/command/start/auth.rs`

The `OidcValidator` performs:

1. **OIDC Discovery:** Fetches `/.well-known/openid-configuration` from the issuer URL. Validates that the discovered issuer matches the configured issuer (normalized, trailing-slash tolerant).
2. **JWKS Fetching:** Retrieves the JSON Web Key Set from the discovered `jwks_uri`. Keys are cached in memory (`Arc<RwLock<JwkSet>>`).
3. **Token Validation:** Decodes the JWT header to extract `kid`, finds the matching RSA key in the JWKS, and validates using `jsonwebtoken` with:
   - Algorithm: RS256 (hardcoded)
   - Audience validation: enabled, against configured `--issuer-audience` values
   - Issuer validation: enabled, accepts with or without trailing slash
   - Expiration (`exp`) validation: enabled
   - Not-before (`nbf`) validation: enabled
4. **Key Rotation:** If `kid` is not found in the cached JWKS, the validator fetches fresh keys once and retries.

**Interceptor pattern:** The interceptor is synchronous (tonic requirement) and uses `tokio::task::block_in_place` + `block_on` to call async validation. Validated `Claims` are inserted into request extensions for downstream handlers.

### 3.2 OAuth2 Device Authorization Flow (CLI Login)

The `vorpal login` command implements the OAuth2 Device Authorization Grant (RFC 8628):

1. Discovers `device_authorization_endpoint` and `token_endpoint` from the issuer's OIDC discovery document.
2. Requests a device code and displays verification URI + user code.
3. Polls the token endpoint until the user completes browser-based authentication.
4. Stores the resulting access token, refresh token, expiry, and scopes in `/var/lib/vorpal/key/credentials.json`.

**Implementation:** `cli/src/command.rs:523-646`

### 3.3 OAuth2 Client Credentials Flow (Service-to-Service)

The worker service uses the Client Credentials Grant for authenticating to the registry when pushing/pulling archives and artifacts on behalf of build requests.

**Implementation:** `cli/src/command/start/auth.rs:314-400` (`exchange_client_credentials`)

Credentials are configured via `--issuer-client-id` and `--issuer-client-secret` CLI flags (passed as plaintext arguments).

### 3.4 Token Refresh

The SDK client (`sdk/rust/src/context.rs:639-682`) implements automatic token refresh:

- Checks if the stored access token has less than 5 minutes remaining.
- Uses the stored refresh token to obtain a new access token via the OIDC token endpoint.
- Updates `/var/lib/vorpal/key/credentials.json` with the new token.
- If no refresh token is available, returns an error directing the user to re-login.

### 3.5 Unauthenticated Mode

When `--issuer` is **not** provided, all services accept requests without any authentication. This is the default for local development using Unix domain sockets. The authorization checks in service handlers are conditional — they only run if `Claims` are present in request extensions.

### 3.6 Identity Provider

The project uses Keycloak as the reference OIDC provider. The `docker-compose.yaml` configures a development Keycloak instance:

```yaml
keycloak:
  command: start-dev
  environment:
    KC_BOOTSTRAP_ADMIN_PASSWORD: password
    KC_BOOTSTRAP_ADMIN_USERNAME: admin
  image: quay.io/keycloak/keycloak:26.5.5
  ports:
    - 127.0.0.1:8080:8080
```

The default `--issuer` for the CLI login command is `http://localhost:8080/realms/vorpal`.

## 4. Authorization

### 4.1 Namespace-Based Permission Model

Authorization is enforced at the gRPC handler level via `require_namespace_permission()` (`cli/src/command/start/auth.rs:405-423`). The model:

- JWT claims include a `namespaces` field: `HashMap<String, Vec<String>>` mapping namespace names to lists of permissions (e.g., `"read"`, `"write"`).
- A wildcard namespace `"*"` grants admin access across all namespaces.
- Each service endpoint checks for the appropriate permission:
  - **archive pull, artifact get, artifact alias get:** `read` permission on the request namespace
  - **archive push, artifact store, worker build:** `write` permission on the request namespace

### 4.2 Authorization Bypass When Auth Disabled

All authorization checks are gated by `if request.extensions().get::<Claims>().is_some()`. When the OIDC interceptor is not configured, `Claims` are never inserted, so authorization is silently skipped. There is no "require auth" mode that rejects unauthenticated requests without a full OIDC setup.

### 4.3 Audit Logging

The `get_user_context()` helper extracts the `sub` claim from validated tokens and logs it alongside operations. This provides basic audit trails for authenticated requests.

**Gap:** Audit logging only occurs for authenticated requests. Unauthenticated operations are logged without user identity.

## 5. Cryptographic Key Management

### 5.1 Key Generation

`vorpal system keys generate` (`cli/src/command/system/keys.rs`) creates the following key material:

| File | Purpose | Algorithm | Generation |
|------|---------|-----------|------------|
| `ca.key.pem` | CA private key | RSA (PKCS_RSA_SHA256 via rcgen) | `KeyPair::generate_for(&PKCS_RSA_SHA256)` |
| `ca.pem` | Self-signed CA certificate | RSA/SHA-256 | `CertificateParams::self_signed` |
| `service.key.pem` | Service private key | RSA (PKCS_RSA_SHA256) | `KeyPair::generate_for(&PKCS_RSA_SHA256)` |
| `service.public.pem` | Service public key (for encryption) | RSA | Extracted from service keypair |
| `service.pem` | Service certificate (signed by CA) | RSA/SHA-256 | Signed by CA, SAN=localhost, ServerAuth EKU |
| `service.secret` | Shared secret (UUID) | N/A | `Uuid::now_v7().to_string()` |

All keys are stored under `/var/lib/vorpal/key/`. Generation is **idempotent** — each file is only created if it does not already exist.

### 5.2 Certificate Properties

- **CA Certificate:** `IsCa::Ca(BasicConstraints::Unconstrained)`, key usages: DigitalSignature, KeyCertSign, CrlSign. DN: C=US, O=Vorpal.
- **Service Certificate:** SAN: `localhost` only. Key usage: DigitalSignature. EKU: ServerAuth. Authority Key Identifier extension enabled.

### 5.3 Notary (Encryption/Decryption for Secrets)

`cli/src/command/store/notary.rs` provides RSA encryption for build secrets:

- **Encrypt:** RSA PKCS1v15 with the service public key. Output is base64-encoded.
- **Decrypt:** RSA PKCS1v15 with the service private key. Input is base64-decoded.

Secrets are encrypted before being embedded in artifact build steps and decrypted by the worker at build time. Decrypted values are injected as environment variables during step execution.

### 5.4 Gaps

- **No key rotation mechanism:** Once generated, keys persist indefinitely. No expiration, rotation schedule, or revocation capability.
- **No file permission restrictions:** Key files are created with default umask permissions. No explicit `chmod 600` on private key files.
- **Service certificate SAN is localhost only:** Remote TLS connections to the service will fail certificate validation unless clients skip hostname verification.
- **Service secret is a UUID v7:** This is used as a shared secret but is a time-based UUID, not a cryptographically random secret. Its entropy is limited.
- **PKCS1v15 padding:** The notary uses PKCS1v15 for encryption, which is older and has known padding oracle vulnerabilities (though exploitation requires an oracle). OAEP would be more modern.

## 6. Secret Management

### 6.1 Build Secrets

Artifact build steps can include encrypted secrets (`step.secrets`). The flow:

1. Secrets are encrypted with the service public key (RSA PKCS1v15) before being included in artifact definitions.
2. During build execution, the worker decrypts each secret using the service private key.
3. Decrypted values are injected as environment variables into the build step's process.

**Implementation:** `cli/src/command/start/worker.rs:479-485`

### 6.2 Credential Storage

User credentials from `vorpal login` are stored in `/var/lib/vorpal/key/credentials.json` as plaintext JSON containing:

- Access tokens
- Refresh tokens
- Client IDs
- Issuer URLs
- Expiry timestamps

**Gap:** Credentials are stored as plaintext on the filesystem with no encryption at rest and no restricted file permissions.

### 6.3 CLI Flags for Secrets

The `--issuer-client-secret` flag passes the OAuth2 client secret as a command-line argument. This makes the secret visible in process listings (`ps aux`) and shell history.

### 6.4 AWS Credentials (S3 Backend)

When using the S3 registry backend, AWS credentials are loaded via the standard AWS SDK credential chain (`aws_config::defaults(BehaviorVersion::latest())`). No custom credential handling — relies on environment variables, instance profiles, or `~/.aws/` configuration.

### 6.5 Environment Variables

Security-relevant environment variables:

| Variable | Purpose |
|----------|---------|
| `VORPAL_SOCKET_PATH` | Override Unix socket path |
| `VORPAL_NONINTERACTIVE` | Installer non-interactive mode |
| `NO_COLOR` | Disable ANSI output |

No secrets are passed via environment variables to the services themselves (they use CLI flags instead).

## 7. Build Execution Security

### 7.1 Sandbox Isolation

Build steps execute as child processes via `tokio::process::Command`. The worker:

- Creates a temporary workspace directory under `/var/lib/vorpal/sandbox/` (UUID-named).
- Sets the working directory to the workspace.
- Injects environment variables (artifact paths, custom envs, decrypted secrets).
- Executes the entrypoint (either a script written to `workspace/script.sh` with `0o755` permissions or a direct binary).

**Gap: No filesystem or process isolation.** Build steps run with the same user privileges as the Vorpal service. There is no container, chroot, namespace isolation, seccomp, or capability dropping. A malicious build step has full access to the host filesystem, network, and all Vorpal key material.

### 7.2 Artifact Integrity

- Artifacts are identified by SHA-256 digest of their JSON-serialized definition.
- Archives are stored as `tar.zst` files and addressed by digest.
- File timestamps are normalized to epoch 0 for reproducibility.

**Gap:** No signature verification on downloaded artifacts. A compromised registry could serve tampered archives. The digest is of the artifact definition, not of the archive contents — there is no content-addressable verification of the actual build output.

### 7.3 Environment Variable Expansion

The `expand_env` function (`cli/src/command/start/worker.rs:373-421`) performs shell-style variable expansion (`$VAR` and `${VAR}`) on build step scripts and arguments. This includes VORPAL_ prefixed vars, custom environments, and decrypted secrets. The expansion is performed in Rust (not via shell), avoiding shell injection, but secrets in environment variables could leak into logs if a build step echoes them.

## 8. Supply Chain Security

### 8.1 Release Artifacts

- Binary releases are built in GitHub Actions CI and published via `softprops/action-gh-release`.
- **Build provenance attestation** is generated using `actions/attest-build-provenance@v4` for all four platform binaries (aarch64-darwin, aarch64-linux, x86_64-darwin, x86_64-linux).
- CI workflow uses `id-token: write` permission for OIDC-based attestation.

### 8.2 NPM SDK Publishing

Uses OIDC Trusted Publishing for the `@altf4llc/vorpal-sdk` package. The workflow (`release-sdk-typescript` job) publishes with `npm publish --provenance --tag next`, which obtains a short-lived OIDC token from GitHub Actions (`id-token: write` permission) and attaches a Sigstore provenance attestation. No long-lived NPM secret is required. A version-existence check prevents duplicate publishes. This was migrated from a legacy `NPM_TOKEN` pattern per `docs/tdd/npm-oidc-trusted-publishing.md`.

### 8.3 Cargo SDK Publishing

Uses a `CARGO_REGISTRY_TOKEN` secret. Crates.io does not yet support OIDC trusted publishing.

### 8.4 Installer Security

The `script/install.sh` installer:

- Downloads pre-built binaries from GitHub Releases over HTTPS.
- **No SHA-256 checksum verification** of downloaded tarballs.
- Verifies basic binary integrity by running `vorpal --version` after extraction.
- Requires `sudo` for creating `/var/lib/vorpal/` directories.
- Supports `curl | bash` pattern with non-interactive mode detection.

### 8.5 Dependency Management

- Rust dependencies managed via `Cargo.lock` (committed to repo).
- Renovate bot configured for automated dependency updates (`.github/workflows/renovate.yaml`).
- Security-critical dependencies: `jsonwebtoken`, `rcgen`, `rsa`, `rustls`, `oauth2`, `aws-sdk-s3`, `tonic` (TLS features).
- HTTP client `reqwest` uses `rustls-tls` feature (not OpenSSL).

### 8.6 .gitignore

The `.gitignore` excludes `.env` and `.env.*` files, preventing accidental commit of environment files that might contain secrets.

## 9. Trust Boundaries

### 9.1 Boundary Map

```
                    +-----------------+
                    |  OIDC Provider  |  (Keycloak / Auth0)
                    |  (external)     |
                    +--------+--------+
                             |
                  OIDC Discovery + JWKS fetch (HTTPS)
                  Device Auth + Token Exchange
                             |
+----------+     +-----------v-----------+     +------------+
|  CLI     |     |  Vorpal Services      |     |  AWS S3    |
|  Client  +---->|  agent | registry |   +---->|  (optional)|
|          |     |  worker              |     |            |
+----------+     +-----------+-----------+     +------------+
  UDS/TCP/TLS                |
                             |
                    +--------v--------+
                    |  Build Steps    |
                    |  (child procs)  |
                    |  NO ISOLATION   |
                    +-----------------+
```

### 9.2 Trust Assumptions

1. **Local machine is trusted:** Default UDS transport assumes the host is single-user or group-restricted.
2. **OIDC provider is trusted:** Token validation trusts the configured issuer entirely. No token introspection or revocation checking.
3. **Build definitions are trusted:** Artifact step scripts execute with full host privileges. There is no policy engine or allowlist for build operations.
4. **Registry content is trusted:** Downloaded artifacts are not signature-verified. Digest matching is against the artifact definition, not archive contents.

## 10. Security Recommendations (Current Gaps Summary)

| Gap | Severity | Category |
|-----|----------|----------|
| Build steps have no process/filesystem isolation | High | Build Security |
| No archive content verification (digest is of definition, not content) | High | Integrity |
| Credentials stored as plaintext in `credentials.json` | Medium | Secret Management |
| No key rotation or expiration mechanism | Medium | Key Management |
| Private key files lack restricted permissions (should be 0600) | Medium | Key Management |
| Client secret passed via CLI flag (visible in `ps`) | Medium | Secret Management |
| PKCS1v15 padding instead of OAEP | Low | Cryptography |
| Service secret is UUID v7 (limited entropy) | Low | Cryptography |
| No SHA-256 checksum verification in installer | Medium | Supply Chain |
| Service cert SAN is localhost only | Low | TLS |
| Health endpoint is always plaintext | Low | Transport |
