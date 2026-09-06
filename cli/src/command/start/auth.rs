use crate::command::{login_http_client, NormalizedIssuer};
use anyhow::{anyhow, bail, Context, Result};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
use tonic::{
    metadata::{Ascii, MetadataValue},
    Request, Status,
};
use tracing::error;
use vorpal_sdk::context::credential_egress_origin;

/// Bounds every OIDC request this file makes, so a hung or hostile endpoint
/// cannot pin a blocking worker for the life of the process: token
/// validation runs inside `block_in_place`, so an unbounded fetch is an
/// availability defect and not only a slow start. Same value as the login
/// path's `LOGIN_HTTP_TIMEOUT`.
const OIDC_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Floor on the time between JWKS refreshes triggered by a token whose `kid`
/// misses the cache.
///
/// The interceptor reaches that refresh before any authentication decision, so
/// without a floor an unauthenticated peer aims one outbound request at the
/// operator's IdP per token by varying `kid`. The bound is a minimum interval
/// rather than a per-`kid` negative cache so a genuine rolling-key rotation is
/// still picked up: it is delayed by at most one interval and never blocked.
/// Five seconds collapses a sustained flood by orders of magnitude while
/// keeping the worst-case rotation delay well inside a token's lifetime.
const MIN_JWKS_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// Turns a redirect into an error instead of a body to parse.
///
/// The client these responses come from refuses to follow redirects, which
/// makes a redirect a returned 3xx rather than a transport error.
/// `error_for_status` does not treat 3xx as failure, so without this the
/// redirect would surface as "parsing discovery doc" and name the wrong
/// cause. Redirects are refused rather than followed because a redirect
/// moves a request *after* the origin pin below has checked the URL.
fn reject_redirect(response: reqwest::Response, url: &str) -> Result<reqwest::Response> {
    if response.status().is_redirection() {
        bail!(
            "refusing to follow redirect ({}) from {}: OIDC endpoints must answer on the \
             issuer's own origin",
            response.status(),
            url
        );
    }

    Ok(response)
}

/// Requires a discovery-named endpoint to share the issuer's
/// `scheme://host:port` origin.
///
/// The discovery document is attacker-influenceable wherever the response
/// bytes are: whoever names `jwks_uri` chooses the keys that validate every
/// bearer token this server accepts, and whoever names `token_endpoint`
/// receives the service-account client secret. The origin comparison is
/// blind to path, which is why the document's own `issuer` claim is still
/// checked separately.
fn require_issuer_origin(endpoint: &str, issuer_origin: &str, field: &str) -> Result<()> {
    let endpoint_origin = credential_egress_origin(endpoint)
        .with_context(|| format!("OIDC discovery {field} {endpoint:?} is not a usable URL"))?;

    if endpoint_origin != issuer_origin {
        bail!("OIDC discovery {field} {endpoint} does not match issuer origin {issuer_origin}");
    }

    Ok(())
}

const RESPONSE_EXCERPT_LIMIT: usize = 512;
const RESPONSE_EXCERPT_TRUNCATION_MARKER: &str = "… (truncated)";

/// Renders an untrusted HTTP response body for a log line or an error.
///
/// Bodies from these endpoints reach `tracing::error!` and, through the
/// returned error, the caller's own `error!` at `worker.rs`. Control
/// characters there forge log records and inject terminal escapes, and an
/// unbounded body copies megabytes into two more strings.
fn response_excerpt(body: &str) -> String {
    let mut printable = body.chars().filter(|character| !character.is_control());
    let mut excerpt: String = printable.by_ref().take(RESPONSE_EXCERPT_LIMIT).collect();

    if printable.next().is_some() {
        excerpt.push_str(RESPONSE_EXCERPT_TRUNCATION_MARKER);
    }

    excerpt
}

#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    jwks_uri: String,
    issuer: String,
}

#[derive(Debug, Deserialize, Clone)]
struct JwkSet {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize, Clone)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    // alg: Option<String>,
    n: Option<String>, // RSA modulus (base64url)
    e: Option<String>, // RSA exponent (base64url)
}

#[derive(Debug, Deserialize, Clone)]
pub struct Claims {
    #[expect(
        dead_code,
        reason = "mirrors the JWT 'aud' claim for Deserialize; not read directly (jsonwebtoken validates audience internally)"
    )]
    pub aud: Option<Value>,
    #[expect(
        dead_code,
        reason = "mirrors the JWT 'exp' claim for Deserialize; not read directly (jsonwebtoken validates expiry internally)"
    )]
    pub exp: Option<u64>,
    #[expect(
        dead_code,
        reason = "mirrors the JWT 'iss' claim for Deserialize; not read directly (jsonwebtoken validates issuer internally)"
    )]
    pub iss: Option<String>,
    pub sub: Option<String>,
    #[expect(
        dead_code,
        reason = "mirrors the JWT 'scope' claim for Deserialize; not currently read by any authorization path"
    )]
    pub scope: Option<String>,
    pub azp: Option<String>,
    pub gty: Option<String>,

    // Namespace permissions
    pub namespaces: Option<HashMap<String, Vec<String>>>,
}

/// Classification of the calling principal, stashed in request extensions
/// alongside `Claims` by the auth interceptor so downstream handlers can
/// distinguish human-user tokens from trusted service-user tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrincipalKind {
    Human,
    TrustedService { azp: String },
}

impl Claims {
    /// Check if user has specific permission for a namespace
    pub fn has_namespace_permission(&self, namespace: &str, permission: &str) -> bool {
        if let Some(ns_perms) = &self.namespaces {
            // Check for exact namespace match
            if let Some(perms) = ns_perms.get(namespace) {
                return perms.contains(&permission.to_string());
            }

            // Check for wildcard admin access
            if let Some(perms) = ns_perms.get("*") {
                return perms.contains(&permission.to_string());
            }
        }

        false
    }

    /// Get subject (user ID or client ID)
    pub fn subject(&self) -> Option<&str> {
        self.sub.as_deref()
    }

    /// Get grant type for audit logging
    #[expect(
        dead_code,
        reason = "public accessor kept for callers doing audit logging; not yet called from this crate"
    )]
    pub fn grant_type(&self) -> Option<&str> {
        self.gty.as_deref()
    }
}

#[derive(Debug, thiserror::Error)]
enum AuthError {
    // #[error("missing authorization header")]
    // MissingAuthHeader,
    #[error("invalid authorization scheme")]
    InvalidScheme,
    #[error("token header missing kid")]
    MissingKid,
    #[error("no matching JWK for kid")]
    KeyNotFound,
    #[error("token validation failed: {0}")]
    Jwt(String),
    // #[error("bad issuer")]
    // Issuer,
    // #[error("bad audience")]
    // Audience,
}

pub struct OidcValidator {
    pub issuer: String,
    pub issuer_audiences: Vec<String>,
    pub jwks_uri: String,
    /// OAuth client IDs whose tokens are classified as `TrustedService` when
    /// seen in the `azp` claim. Empty by default — callers opt in via
    /// [`OidcValidator::with_trusted_service_client_ids`] so the existing
    /// `OidcValidator::new` signature remains backward compatible.
    pub trusted_service_client_ids: Vec<String>,
    // Cache the current JWK set; refresh if key not found
    jwks: Arc<RwLock<JwkSet>>,
    /// The one hardened client every OIDC fetch goes through, including the
    /// rolling-key refresh an unauthenticated peer can trigger.
    client: reqwest::Client,
    /// Instant of the last *attempted* cache-miss refresh, `None` until the
    /// first one. Attempted rather than successful: keying on success would
    /// hand the unbounded rate back whenever the IdP is failing, which is
    /// exactly when the extra load is least affordable.
    ///
    /// A `Mutex` rather than an `RwLock` because the test-and-claim and the
    /// fetch it authorizes must be one atomic transition: the guard is held
    /// across the `await` so C concurrent misses issue one fetch, not C. It is
    /// a `tokio` mutex for that reason — a `std` one held across an await
    /// would block the executor thread outright.
    last_refresh_attempt: Mutex<Option<Instant>>,
    min_refresh_interval: Duration,
}

impl OidcValidator {
    pub async fn new(issuer: String, issuer_audiences: Vec<String>) -> Result<Self> {
        // The issuer is validated here, at the chokepoint, and not only by
        // the `--issuer` value parser: this constructor is `pub` and takes a
        // bare `String`, so a caller added later would otherwise reintroduce
        // a plaintext non-loopback trust anchor with no error. Parsing also
        // canonicalizes, which is what makes the equality below sound.
        let issuer = NormalizedIssuer::parse(&issuer)
            .map_err(|cause| anyhow!("invalid OIDC issuer {issuer:?}: {cause}"))?;
        let issuer_origin = credential_egress_origin(issuer.as_str())?;
        let client = login_http_client(OIDC_HTTP_TIMEOUT)?;

        // 1) Discover the realm (/.well-known/openid-configuration)
        let discovery_url = format!("{}/.well-known/openid-configuration", issuer);
        let disc: OidcDiscovery =
            reject_redirect(client.get(&discovery_url).send().await?, &discovery_url)?
                .error_for_status()?
                .json()
                .await
                .context("parsing discovery doc")?;

        // Both sides are normalized: an IdP that states its `iss` with an
        // explicit default port or a mixed-case host means the same issuer,
        // and comparing raw text there fails a correct deployment closed.
        let disc_issuer = NormalizedIssuer::parse(&disc.issuer).map_err(|cause| {
            anyhow!(
                "OIDC discovery issuer {:?} is invalid: {cause}",
                disc.issuer
            )
        })?;

        if disc_issuer != issuer {
            // Path-blind origin pinning alone would accept a co-tenant's
            // endpoints on a multi-tenant IdP; this is the check that does not.
            return Err(anyhow!(
                "issuer mismatch (expected {}, discovery says {})",
                issuer,
                disc.issuer
            ));
        }

        require_issuer_origin(&disc.jwks_uri, &issuer_origin, "jwks_uri")?;

        // 2) Fetch JWKS
        let jwks = fetch_jwks(&client, &disc.jwks_uri).await?;

        Ok(Self {
            issuer: issuer.as_str().to_string(),
            issuer_audiences,
            jwks: Arc::new(RwLock::new(jwks)),
            // The pinned URL is what gets stored, so the refresh below reuses
            // a value that was checked rather than re-deriving one.
            jwks_uri: disc.jwks_uri,
            trusted_service_client_ids: Vec::new(),
            client,
            last_refresh_attempt: Mutex::new(None),
            min_refresh_interval: MIN_JWKS_REFRESH_INTERVAL,
        })
    }

    /// Builder: set the list of OAuth client IDs whose tokens should be
    /// classified as `TrustedService`. Added as a post-construction setter to
    /// keep the existing `OidcValidator::new(issuer, audiences)` call sites
    /// compiling unchanged; called from `start.rs` to thread the
    /// `--issuer-service-client-ids` CLI flag through to the interceptor.
    pub fn with_trusted_service_client_ids(mut self, ids: Vec<String>) -> Self {
        self.trusted_service_client_ids = ids;
        self
    }

    /// Overrides the cache-miss refresh floor so a test can exercise both
    /// sides of the bound without waiting out the production default.
    #[cfg(test)]
    fn with_min_refresh_interval(mut self, interval: Duration) -> Self {
        self.min_refresh_interval = interval;
        self
    }

    async fn validate(&self, bearer: &str) -> Result<Claims, AuthError> {
        let token = bearer
            .strip_prefix("Bearer ")
            .ok_or(AuthError::InvalidScheme)?;

        // Decode header to pick the right key (kid)
        let header = decode_header(token).map_err(|e| AuthError::Jwt(e.to_string()))?;
        let kid = header.kid.ok_or(AuthError::MissingKid)?;
        let aud: Vec<&str> = self
            .issuer_audiences
            .iter()
            .map(std::string::String::as_str)
            .collect();

        // Try current cache
        if let Some(claims) = self.try_decode_with_kid(&aud, &kid, token).await? {
            // self.validate_claims(&claims)?;
            return Ok(claims);
        }

        // If not found, refresh JWKS once and retry (handles rolling keys).
        self.refresh_jwks_if_interval_elapsed().await?;

        if let Some(claims) = self.try_decode_with_kid(&aud, &kid, token).await? {
            // self.validate_claims(&claims)?;
            return Ok(claims);
        }

        Err(AuthError::KeyNotFound)
    }

    /// Fetches the JWK set and replaces the cache, unless a refresh was already
    /// attempted less than [`Self::min_refresh_interval`] ago.
    ///
    /// A suppressed refresh is not an error: the caller falls through to the
    /// same `KeyNotFound` it would have returned had the fetch happened and
    /// still not produced the `kid`. Only a refresh that ran and failed is
    /// reported, which keeps a broken IdP diagnosable.
    async fn refresh_jwks_if_interval_elapsed(&self) -> Result<(), AuthError> {
        let mut last_attempt = self.last_refresh_attempt.lock().await;

        if let Some(attempted_at) = *last_attempt {
            if attempted_at.elapsed() < self.min_refresh_interval {
                return Ok(());
            }
        }

        *last_attempt = Some(Instant::now());

        let fresh = fetch_jwks(&self.client, &self.jwks_uri)
            .await
            .map_err(|e| AuthError::Jwt(format!("jwks refresh failed: {e}")))?;

        *self.jwks.write().await = fresh;

        Ok(())
    }

    async fn try_decode_with_kid(
        &self,
        aud: &[&str],
        kid: &str,
        token: &str,
    ) -> Result<Option<Claims>, AuthError> {
        // Cloned to release the JWKS read lock before the (potentially slow) decode
        // below runs, rather than holding the lock across it.
        let jwks = self.jwks.read().await.clone();
        let Some(jwk) = jwks.keys.iter().find(|k| k.kid.as_deref() == Some(kid)) else {
            return Ok(None);
        };

        // We assume RS256 (Keycloak default for access tokens)
        if jwk.kty != "RSA" {
            return Err(AuthError::Jwt("unsupported kty".into()));
        }
        let n = jwk
            .n
            .as_deref()
            .ok_or_else(|| AuthError::Jwt("missing n".into()))?;
        let e = jwk
            .e
            .as_deref()
            .ok_or_else(|| AuthError::Jwt("missing e".into()))?;

        let key =
            DecodingKey::from_rsa_components(n, e).map_err(|e| AuthError::Jwt(e.to_string()))?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(aud);
        validation.validate_aud = true;
        // Accept issuer with or without trailing slash (Auth0 includes it, others may not)
        validation.set_issuer(&[&self.issuer, &format!("{}/", self.issuer)]);
        validation.validate_exp = true;
        validation.validate_nbf = true;

        let data = decode::<Claims>(token, &key, &validation)
            .map_err(|e| AuthError::Jwt(e.to_string()))?;

        Ok(Some(data.claims))
    }

    // fn validate_claims(&self, claims: &Claims) -> Result<(), AuthError> {
    //     // jsonwebtoken already enforced iss/aud/exp/nbf via Validation above.
    //     // Keep these guardrails in case Validation config changes.
    //     if claims.iss.as_deref() != Some(&self.issuer) {
    //         return Err(AuthError::Issuer);
    //     }
    //     // aud can be string or array in OIDC
    //     let aud_ok = match &claims.aud {
    //         Some(serde_json::Value::String(s)) => s == &self.expected_aud,
    //         Some(serde_json::Value::Array(v)) => {
    //             v.iter().any(|x| x.as_str() == Some(&self.expected_aud))
    //         }
    //         _ => false,
    //     };
    //     if !aud_ok {
    //         return Err(AuthError::Audience);
    //     }
    //     Ok(())
    // }
}

async fn fetch_jwks(client: &reqwest::Client, uri: &str) -> Result<JwkSet> {
    let jwks: JwkSet = reject_redirect(client.get(uri).send().await?, uri)?
        .error_for_status()?
        .json()
        .await?;
    Ok(jwks)
}

// ===== Interceptor =====

/// Classify a validated token's principal by checking whether its `azp`
/// (authorized party) claim matches the trusted service client-ID allow-list.
///
/// Extracted from `new_interceptor` so the classification rule can be unit
/// tested without constructing an `OidcValidator` (which performs network I/O
/// for OIDC discovery).
fn classify_principal(azp: Option<&str>, trusted_service_client_ids: &[String]) -> PrincipalKind {
    match azp {
        Some(value) if trusted_service_client_ids.iter().any(|id| id == value) => {
            PrincipalKind::TrustedService {
                azp: value.to_string(),
            }
        }
        _ => PrincipalKind::Human,
    }
}

pub fn new_interceptor(
    validator: Arc<OidcValidator>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |mut req: Request<()>| {
        // Read "authorization" metadata (lowercase in gRPC/HTTP2)
        let auth = req
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("missing authorization"))?
            .to_string();

        // We need async validation; Interceptor is sync. Workaround: block_in_place.
        // For high-throughput, prefer a tower layer that supports async, but this is simple & fine.
        // Clone once into `validator_for_validate` for the async move; the outer
        // `validator` Arc (captured by the `Fn` closure) remains usable afterward
        // via shared borrow for `trusted_service_client_ids` access — no second
        // clone needed.
        let validator_for_validate = Arc::clone(&validator);
        let claims = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current()
                .block_on(async move { validator_for_validate.validate(&auth).await })
        })
        .map_err(|e| Status::unauthenticated(format!("token invalid: {e}")))?;

        let principal =
            classify_principal(claims.azp.as_deref(), &validator.trusted_service_client_ids);

        // Stash claims + classified principal for handlers
        req.extensions_mut().insert(claims);
        req.extensions_mut().insert(principal);
        Ok(req)
    }
}

// ===== OAuth2 Client Credentials Flow =====

#[derive(Debug, Deserialize)]
struct TokenEndpointDiscovery {
    /// Required rather than `Option`, so a document that omits the claim
    /// fails the parse instead of skipping the compare below.
    issuer: String,
    token_endpoint: String,
}

#[derive(Debug, Serialize)]
struct ClientCredentialsRequest {
    audience: Option<String>,
    client_id: String,
    client_secret: String,
    grant_type: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct ClientCredentialsResponse {
    access_token: String,
    expires_in: u64,
    #[expect(
        dead_code,
        reason = "mirrors the OAuth2 token endpoint response for Deserialize; the token type (always 'Bearer') is not read"
    )]
    token_type: String,
}

/// Compose the `OAuth2` scope string for a `client_credentials` request.
///
/// When `issuer_audience` is provided, appends Zitadel's project-audience-injection
/// scope (`urn:zitadel:iam:org:project:id:{aud}:aud`). Zitadel v4 silently ignores
/// the `audience` form-body parameter for the client-credentials grant — the URN
/// scope is the only way to force the project ID into the minted token's `aud`.
/// Keycloak and Auth0 ignore unrecognized scope strings, so the appended URN is
/// harmless against them.
fn compose_client_credentials_scope(scope: &str, issuer_audience: Option<&str>) -> String {
    match issuer_audience {
        Some(aud) => format!("{scope} urn:zitadel:iam:org:project:id:{aud}:aud"),
        None => scope.to_string(),
    }
}

/// Performs `OAuth2` Client Credentials Flow token exchange for service-to-service authentication
///
/// Args:
/// - issuer: Base URL of the OIDC provider (e.g., <http://localhost:8080/realms/vorpal>)
/// - `client_id`: Service account client ID
/// - `client_secret`: Service account client secret
/// - scope: `OAuth2` scope to request
/// - audience: API identifier
///
/// Returns: Bearer token as `MetadataValue` and `expires_in` suitable for gRPC requests
pub async fn exchange_client_credentials(
    issuer: &str,
    issuer_audience: Option<&str>,
    issuer_client_id: &str,
    issuer_client_secret: &str,
    scope: &str,
) -> Result<(MetadataValue<Ascii>, u64)> {
    // 1) Discover the token endpoint via OIDC discovery
    let issuer = NormalizedIssuer::parse(issuer)
        .map_err(|cause| anyhow!("invalid OIDC issuer {issuer:?}: {cause}"))?;
    let issuer_origin = credential_egress_origin(issuer.as_str())?;
    let client = login_http_client(OIDC_HTTP_TIMEOUT)?;

    let discovery_url = format!("{}/.well-known/openid-configuration", issuer);

    let discovery_response = reject_redirect(
        client
            .get(&discovery_url)
            .send()
            .await
            .context("failed to fetch OIDC discovery")?,
        &discovery_url,
    )?
    .error_for_status()
    .context("OIDC discovery request failed")?;

    // let discovery_status = discovery_response.status();
    let discovery_text = discovery_response
        .text()
        .await
        .unwrap_or_else(|_| "<unable to read response body>".to_string());

    let disc: TokenEndpointDiscovery = serde_json::from_str(&discovery_text).map_err(|e| {
        let excerpt = response_excerpt(&discovery_text);

        error!(
            "auth |> failed to parse OIDC discovery response: {} - full response: {}",
            e, excerpt
        );
        anyhow!("failed to parse OIDC discovery response: {e} - response was: {excerpt}")
    })?;

    // Both sides are normalized, mirroring `OidcValidator::new`: an IdP that
    // states its `iss` with an explicit default port or a trailing slash
    // means the same issuer, and comparing raw text fails a correct
    // deployment closed.
    let disc_issuer = NormalizedIssuer::parse(&disc.issuer).map_err(|cause| {
        anyhow!(
            "OIDC discovery issuer {:?} is invalid: {cause}",
            disc.issuer
        )
    })?;

    if disc_issuer != issuer {
        // The origin pin below is blind to path, so a co-tenant realm
        // answering on the issuer's own origin would otherwise receive the
        // service-account client secret.
        return Err(anyhow!(
            "issuer mismatch (expected {}, discovery says {})",
            issuer,
            disc.issuer
        ));
    }

    // The POST below carries the service-account client secret, so an
    // off-origin `token_endpoint` is credential exfiltration and not merely
    // a misrouted request.
    require_issuer_origin(&disc.token_endpoint, &issuer_origin, "token_endpoint")?;

    // 2) Exchange client credentials for access token
    let token_request = ClientCredentialsRequest {
        audience: issuer_audience.map(std::string::ToString::to_string),
        client_id: issuer_client_id.to_string(),
        client_secret: issuer_client_secret.to_string(),
        grant_type: "client_credentials".to_string(),
        scope: compose_client_credentials_scope(scope, issuer_audience),
    };

    // A 307 or 308 replays the request body — the client secret — at the
    // redirect target, which header stripping does not prevent. Refusing the
    // redirect is the control.
    let token_response = reject_redirect(
        client
            .post(&disc.token_endpoint)
            .form(&token_request)
            .send()
            .await
            .context("failed to send token request to OIDC provider")?,
        &disc.token_endpoint,
    )?;

    let token_status = token_response.status();
    let token_text = token_response
        .text()
        .await
        .unwrap_or_else(|_| "<unable to read response body>".to_string());

    if !token_status.is_success() {
        let excerpt = response_excerpt(&token_text);

        error!(
            "auth |> token exchange failed with status {}: {}",
            token_status, excerpt
        );
        return Err(anyhow!("token endpoint returned {token_status}: {excerpt}"));
    }

    let response: ClientCredentialsResponse = serde_json::from_str(&token_text).map_err(|e| {
        let excerpt = response_excerpt(&token_text);

        error!(
            "auth |> failed to parse token response: {} - full response: {}",
            e, excerpt
        );
        anyhow!("failed to parse token response: {e} - response was: {excerpt}")
    })?;

    // 3) Create Bearer token header
    let auth_header: MetadataValue<Ascii> = format!("Bearer {}", response.access_token)
        .parse()
        .map_err(|e| anyhow!("failed to parse Bearer token: {e}"))?;

    Ok((auth_header, response.expires_in))
}

// ===== Authorization Helpers =====

/// Require namespace permission in gRPC handler - returns 403 if missing
pub fn require_namespace_permission<T>(
    request: &Request<T>,
    namespace: &str,
    permission: &str,
) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<Claims>()
        .ok_or_else(|| Status::unauthenticated("no claims found"))?;

    if !claims.has_namespace_permission(namespace, permission) {
        return Err(Status::permission_denied(format!(
            "insufficient permissions: no {permission} access to namespace: {namespace}"
        )));
    }

    Ok(())
}

/// Authorization gate that splits on principal kind: trusted service tokens
/// bypass namespace RBAC entirely; human tokens delegate to
/// [`require_namespace_permission`], preserving today's behavior. The
/// interceptor must have classified the principal into `PrincipalKind` in
/// request extensions before this runs; missing classification is treated as
/// `UNAUTHENTICATED` rather than silently falling back.
pub fn require_namespace_or_service_trust<T>(
    request: &Request<T>,
    namespace: &str,
    permission: &str,
) -> Result<(), Status> {
    let principal = request
        .extensions()
        .get::<PrincipalKind>()
        .ok_or_else(|| Status::unauthenticated("no principal found"))?;

    match principal {
        PrincipalKind::TrustedService { .. } => Ok(()),
        PrincipalKind::Human => require_namespace_permission(request, namespace, permission),
    }
}

/// Extract user context for audit logging
pub fn get_user_context<T>(request: &Request<T>) -> Option<String> {
    request
        .extensions()
        .get::<Claims>()
        .and_then(|claims| claims.subject().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_scope_without_audience_is_unchanged() {
        let result = compose_client_credentials_scope("read:archive write:archive", None);
        assert_eq!(result, "read:archive write:archive");
    }

    #[test]
    fn compose_scope_with_numeric_zitadel_audience_appends_urn() {
        let result = compose_client_credentials_scope(
            "read:archive write:archive",
            Some("368890692711219611"),
        );
        assert_eq!(
            result,
            "read:archive write:archive urn:zitadel:iam:org:project:id:368890692711219611:aud"
        );
    }

    #[test]
    fn compose_scope_with_non_numeric_audience_still_appends_urn() {
        // Unconditional append: Keycloak/Auth0 ignore unrecognized scope strings,
        // so even a slug or URL audience is passed through without harm.
        let result = compose_client_credentials_scope("openid", Some("vorpal"));
        assert_eq!(result, "openid urn:zitadel:iam:org:project:id:vorpal:aud");
    }

    #[test]
    fn compose_scope_preserves_base_scope_order() {
        let result = compose_client_credentials_scope("a b c", Some("42"));
        assert!(result.starts_with("a b c "));
        assert!(result.ends_with(" urn:zitadel:iam:org:project:id:42:aud"));
    }

    // ===== Principal classification (TDD §10.1 items 1–4) =====

    #[test]
    fn principal_classification_human_when_allow_list_empty() {
        let result = classify_principal(Some("any-client"), &[]);
        assert_eq!(result, PrincipalKind::Human);
    }

    #[test]
    fn principal_classification_human_when_azp_missing() {
        let result = classify_principal(None, &["worker-client".to_string()]);
        assert_eq!(result, PrincipalKind::Human);
    }

    #[test]
    fn principal_classification_trusted_when_azp_matches() {
        let result = classify_principal(
            Some("worker-client"),
            &["other".to_string(), "worker-client".to_string()],
        );
        assert_eq!(
            result,
            PrincipalKind::TrustedService {
                azp: "worker-client".to_string()
            }
        );
    }

    #[test]
    fn principal_classification_case_sensitive() {
        // Providers emit client IDs verbatim — do not lower-case either side.
        let result = classify_principal(Some("Worker-Client"), &["worker-client".to_string()]);
        assert_eq!(result, PrincipalKind::Human);
    }

    // ===== require_namespace_or_service_trust (TDD §10.1 items 5–8) =====

    fn mk_claims(namespaces: Option<HashMap<String, Vec<String>>>) -> Claims {
        Claims {
            aud: None,
            exp: None,
            iss: None,
            sub: Some("test-subject".to_string()),
            scope: None,
            azp: None,
            gty: None,
            namespaces,
        }
    }

    #[test]
    fn require_namespace_or_service_trust_human_with_permission_passes() {
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["read".to_string()]);
        let mut req = Request::new(());
        req.extensions_mut().insert(mk_claims(Some(ns)));
        req.extensions_mut().insert(PrincipalKind::Human);

        let result = require_namespace_or_service_trust(&req, "library", "read");
        assert!(result.is_ok());
    }

    #[test]
    fn require_namespace_or_service_trust_human_without_permission_fails(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut req = Request::new(());
        req.extensions_mut().insert(mk_claims(Some(HashMap::new())));
        req.extensions_mut().insert(PrincipalKind::Human);

        let Err(err) = require_namespace_or_service_trust(&req, "library", "read") else {
            return Err("should be denied".into());
        };
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        // TDD §6 error taxonomy: the wire-format message must be EXACTLY
        // "insufficient permissions: no {perm} access to namespace: {ns}".
        // External tooling and DKT-67 scenario 4 (unknown azp) assert against
        // this string — a substring check would mask drift. Keep verbatim.
        assert_eq!(
            err.message(),
            "insufficient permissions: no read access to namespace: library"
        );
        Ok(())
    }

    #[test]
    fn require_namespace_or_service_trust_human_partial_permission_other_namespace_fails(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // DKT-67 scenario 2 (regression): a human user with `library:write`
        // attempting to build into `other` must hit the EXACT TDD §6 error
        // string for `write` on `other`. Lock the message format here so any
        // refactor of the error path is caught at the unit boundary, not at
        // the live-Keycloak harness.
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["write".to_string()]);
        let mut req = Request::new(());
        req.extensions_mut().insert(mk_claims(Some(ns)));
        req.extensions_mut().insert(PrincipalKind::Human);

        let Err(err) = require_namespace_or_service_trust(&req, "other", "write") else {
            return Err("should be denied".into());
        };
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no write access to namespace: other"
        );
        Ok(())
    }

    #[test]
    fn require_namespace_or_service_trust_human_partial_permission_same_namespace_passes() {
        // Companion to the regression test above: same human user, same claim,
        // but building into `library` (the namespace they DO have `write` on).
        // Confirms the gate is permission+namespace-keyed, not flag-keyed.
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["write".to_string()]);
        let mut req = Request::new(());
        req.extensions_mut().insert(mk_claims(Some(ns)));
        req.extensions_mut().insert(PrincipalKind::Human);

        let result = require_namespace_or_service_trust(&req, "library", "write");
        assert!(result.is_ok());
    }

    #[test]
    fn require_namespace_or_service_trust_trusted_always_passes() {
        // No Claims needed on the TrustedService branch — it short-circuits.
        let mut req = Request::new(());
        req.extensions_mut().insert(PrincipalKind::TrustedService {
            azp: "vorpal-worker".to_string(),
        });

        let result = require_namespace_or_service_trust(&req, "any-namespace", "write");
        assert!(result.is_ok());
    }

    #[test]
    fn require_namespace_or_service_trust_no_principal_fails_unauthenticated(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let req: Request<()> = Request::new(());

        let Err(err) = require_namespace_or_service_trust(&req, "library", "read") else {
            return Err("should be unauthenticated".into());
        };
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
        Ok(())
    }

    // ===== End-to-end gate composition: classify + gate (DKT-67 scenarios) =====
    //
    // These tests stitch `classify_principal` and `require_namespace_or_service_trust`
    // together against a hand-built request (the interceptor would normally do
    // this composition, but it does network I/O for token validation — see
    // `script/test/integration/m2m-authz.sh` for the live-Keycloak coverage).
    // The goal here is to lock the *combined* behavior so that a refactor that
    // moves the classify call (e.g. into a layer) cannot silently break the
    // five DKT-67 acceptance scenarios.

    fn build_request_with_claims_and_classification(
        claims: Claims,
        trusted_service_client_ids: &[String],
    ) -> Request<()> {
        let principal = classify_principal(claims.azp.as_deref(), trusted_service_client_ids);
        let mut req = Request::new(());
        req.extensions_mut().insert(claims);
        req.extensions_mut().insert(principal);
        req
    }

    fn mk_claims_with_azp(
        azp: Option<&str>,
        namespaces: Option<HashMap<String, Vec<String>>>,
    ) -> Claims {
        Claims {
            aud: None,
            exp: None,
            iss: None,
            sub: Some("test-subject".to_string()),
            scope: None,
            azp: azp.map(str::to_string),
            gty: None,
            namespaces,
        }
    }

    #[test]
    fn dkt67_scenario1_keycloak_happy_path_service_account_bypasses_rbac() {
        // Service-account token: azp = worker-client, no `namespaces` claim,
        // worker-client IS in the trusted allow-list. Worker `build_artifact`
        // call must succeed without RBAC.
        let claims = mk_claims_with_azp(Some("worker-client"), None);
        let req =
            build_request_with_claims_and_classification(claims, &["worker-client".to_string()]);

        let result = require_namespace_or_service_trust(&req, "library", "write");
        assert!(result.is_ok(), "trusted service must bypass: {result:?}");
    }

    #[test]
    fn dkt67_scenario2_keycloak_regression_human_with_namespaces_succeeds_in_owned_ns() {
        // Human-user token, NO trusted-service flag (allow-list empty),
        // namespaces claim grants `library:write`. Build in `library` succeeds.
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["write".to_string()]);
        let claims = mk_claims_with_azp(Some("cli"), Some(ns));
        let req = build_request_with_claims_and_classification(claims, &[]);

        let result = require_namespace_or_service_trust(&req, "library", "write");
        assert!(
            result.is_ok(),
            "human with `library:write` must succeed in `library`: {result:?}"
        );
    }

    #[test]
    fn dkt67_scenario2_keycloak_regression_human_with_namespaces_fails_in_unowned_ns(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Companion: same human, build in `other` → PERMISSION_DENIED with the
        // exact TDD §6 error string.
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["write".to_string()]);
        let claims = mk_claims_with_azp(Some("cli"), Some(ns));
        let req = build_request_with_claims_and_classification(claims, &[]);

        let Err(err) = require_namespace_or_service_trust(&req, "other", "write") else {
            return Err("human without `other:write` must be denied".into());
        };
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no write access to namespace: other"
        );
        Ok(())
    }

    #[test]
    fn dkt67_scenario4_unknown_azp_returns_permission_denied_with_exact_error_string(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Negative — token from a service account whose `azp` is NOT in the
        // allow-list. The flag is set (worker-client trusted), but this token's
        // `azp` is `attacker-client`, which falls back to the Human path.
        // Without `namespaces`, the gate denies with the EXACT TDD §6 string.
        let claims = mk_claims_with_azp(Some("attacker-client"), None);
        let req =
            build_request_with_claims_and_classification(claims, &["worker-client".to_string()]);

        let Err(err) = require_namespace_or_service_trust(&req, "library", "read") else {
            return Err("unknown azp must be denied".into());
        };
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no read access to namespace: library"
        );
        Ok(())
    }

    #[test]
    fn dkt67_scenario4_unknown_azp_with_partial_namespaces_still_namespace_gated(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Variant: unknown azp + a `namespaces` claim that doesn't cover the
        // requested ns → still PERMISSION_DENIED. Confirms the bypass is NOT
        // triggered just because the token has an `azp` field; the allow-list
        // membership is the ONLY classifier into TrustedService.
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["read".to_string()]);
        let claims = mk_claims_with_azp(Some("attacker-client"), Some(ns));
        let req =
            build_request_with_claims_and_classification(claims, &["worker-client".to_string()]);

        let Err(err) = require_namespace_or_service_trust(&req, "library", "write") else {
            return Err("unknown azp without `library:write` must be denied".into());
        };
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no write access to namespace: library"
        );
        Ok(())
    }

    #[test]
    fn dkt67_scenario4_trusted_azp_supersedes_missing_namespace_claim() {
        // Inverse of the unknown-azp test: trusted azp + missing namespaces
        // claim must still bypass — covering the worker-on-Zitadel case
        // (TDD §1.1, the operational break this whole TDD fixes).
        let claims = mk_claims_with_azp(Some("worker-client"), None);
        let req =
            build_request_with_claims_and_classification(claims, &["worker-client".to_string()]);

        let result = require_namespace_or_service_trust(&req, "any-ns", "write");
        assert!(
            result.is_ok(),
            "trusted azp must bypass even with no namespaces claim: {result:?}"
        );
    }

    // ===== OIDC key provenance (VPL-699) =====
    //
    // The network is the only thing faked here: the code under test is the
    // real discovery fetch, the real origin checks and the real token
    // exchange. Every negative test below is paired with a positive control
    // in the same module, because "the off-origin fixture logged zero
    // requests" and "the fixture never worked" are the same observation
    // otherwise.

    /// A minimal HTTP/1.1 stand-in for an IdP, answering each request with
    /// whatever `respond` returns for that request's path and recording the
    /// paths it served.
    struct IdpServer {
        addr: std::net::SocketAddr,
        paths: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl IdpServer {
        async fn start(
            respond: impl Fn(&str, std::net::SocketAddr) -> String + Send + Sync + 'static,
        ) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind idp fixture");
            let addr = listener.local_addr().expect("fixture address");
            let paths = Arc::new(std::sync::Mutex::new(Vec::new()));
            let respond = Arc::new(respond);
            let accepted = paths.clone();

            tokio::spawn(async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        return;
                    };

                    let respond = respond.clone();
                    let accepted = accepted.clone();

                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};

                        let mut buffer = vec![0u8; 8192];
                        let read = socket.read(&mut buffer).await.unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or_default()
                            .to_string();

                        accepted.lock().unwrap().push(path.clone());

                        let response = respond(&path, addr);

                        let _ = socket.write_all(response.as_bytes()).await;
                    });
                }
            });

            Self { addr, paths }
        }

        fn issuer(&self) -> String {
            format!("http://127.0.0.1:{}", self.addr.port())
        }

        fn requested_paths(&self) -> Vec<String> {
            self.paths.lock().unwrap().clone()
        }
    }

    fn http_json(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn http_redirect(status: &str, location: &str) -> String {
        format!(
            "HTTP/1.1 {}\r\nlocation: {}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            status, location
        )
    }

    fn discovery_document(issuer: &str, jwks_uri: &str) -> String {
        http_json(&format!(
            "{{\"issuer\":\"{}\",\"jwks_uri\":\"{}\",\"token_endpoint\":\"{}/token\"}}",
            issuer, jwks_uri, issuer
        ))
    }

    const EMPTY_JWKS: &str = "{\"keys\":[]}";

    #[tokio::test]
    async fn oidc_validator_accepts_same_origin_discovery() {
        // Positive control for both refusal tests below: without it, their
        // "zero requests" assertions could pass against an inert fixture.
        let idp = IdpServer::start(|path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            match path {
                "/.well-known/openid-configuration" => {
                    discovery_document(&issuer, &format!("{}/jwks", issuer))
                }
                _ => http_json(EMPTY_JWKS),
            }
        })
        .await;

        let validator = OidcValidator::new(idp.issuer(), vec![])
            .await
            .expect("same-origin discovery must succeed");

        assert_eq!(validator.issuer, idp.issuer());
        assert_eq!(validator.jwks_uri, format!("{}/jwks", idp.issuer()));
        assert_eq!(
            idp.requested_paths(),
            vec![
                "/.well-known/openid-configuration".to_string(),
                "/jwks".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn oidc_validator_refuses_redirected_discovery() {
        let elsewhere = IdpServer::start(|path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            match path {
                "/.well-known/openid-configuration" => {
                    discovery_document(&issuer, &format!("{}/jwks", issuer))
                }
                _ => http_json(EMPTY_JWKS),
            }
        })
        .await;

        let target = elsewhere.issuer();
        let idp = IdpServer::start(move |_, _| {
            http_redirect(
                "302 Found",
                &format!("{}/.well-known/openid-configuration", target),
            )
        })
        .await;

        let err = OidcValidator::new(idp.issuer(), vec![])
            .await
            .err()
            .expect("a redirected discovery response must be refused");

        assert!(
            err.to_string().contains("refusing to follow redirect"),
            "unexpected error: {err}"
        );
        assert!(
            elsewhere.requested_paths().is_empty(),
            "the redirect target must never be contacted, got {:?}",
            elsewhere.requested_paths()
        );
    }

    #[tokio::test]
    async fn oidc_validator_refuses_cross_origin_jwks_uri() {
        let elsewhere = IdpServer::start(|_, _| http_json(EMPTY_JWKS)).await;

        let attacker_jwks = format!("{}/jwks", elsewhere.issuer());
        let idp = IdpServer::start(move |path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            match path {
                // The document's own `issuer` claim is honest; only
                // `jwks_uri` points elsewhere.
                "/.well-known/openid-configuration" => discovery_document(&issuer, &attacker_jwks),
                _ => http_json(EMPTY_JWKS),
            }
        })
        .await;

        let err = OidcValidator::new(idp.issuer(), vec![])
            .await
            .err()
            .expect("an off-origin jwks_uri must be refused");

        assert!(
            err.to_string().contains("jwks_uri")
                && err.to_string().contains("does not match issuer origin"),
            "unexpected error: {err}"
        );
        assert!(
            elsewhere.requested_paths().is_empty(),
            "the off-origin key host must never be contacted, got {:?}",
            elsewhere.requested_paths()
        );
    }

    #[tokio::test]
    async fn oidc_validator_refuses_non_loopback_plaintext_issuer() {
        // No fixture: the issuer must be refused before any network I/O, so
        // the error names the scheme rule rather than a failed connection.
        let err = OidcValidator::new("http://idp.internal/realms/vorpal".to_string(), vec![])
            .await
            .err()
            .expect("a plaintext non-loopback issuer must be refused");

        assert!(
            err.to_string().contains("must be https"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn client_credentials_exchange_succeeds_on_issuer_origin() {
        // Positive control for the two token-endpoint refusals below.
        let idp = IdpServer::start(|path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            match path {
                "/.well-known/openid-configuration" => {
                    discovery_document(&issuer, &format!("{}/jwks", issuer))
                }
                _ => http_json(
                    "{\"access_token\":\"header.payload.signature\",\"expires_in\":300,\
                     \"token_type\":\"Bearer\"}",
                ),
            }
        })
        .await;

        let (header, expires_in) =
            exchange_client_credentials(&idp.issuer(), None, "vorpal-worker", "s3cr3t", "openid")
                .await
                .expect("same-origin token exchange must succeed");

        assert_eq!(header.to_str().unwrap(), "Bearer header.payload.signature");
        assert_eq!(expires_in, 300);
        assert!(idp.requested_paths().contains(&"/token".to_string()));
    }

    #[tokio::test]
    async fn client_credentials_exchange_refuses_cross_origin_token_endpoint() {
        let elsewhere = IdpServer::start(|_, _| http_json("{}")).await;

        let attacker_token_endpoint = format!("{}/token", elsewhere.issuer());
        let idp = IdpServer::start(move |_, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            http_json(&format!(
                "{{\"issuer\":\"{}\",\"jwks_uri\":\"{}/jwks\",\"token_endpoint\":\"{}\"}}",
                issuer, issuer, attacker_token_endpoint
            ))
        })
        .await;

        let err =
            exchange_client_credentials(&idp.issuer(), None, "vorpal-worker", "s3cr3t", "openid")
                .await
                .expect_err("an off-origin token_endpoint must be refused");

        assert!(
            err.to_string().contains("token_endpoint")
                && err.to_string().contains("does not match issuer origin"),
            "unexpected error: {err}"
        );
        assert!(
            elsewhere.requested_paths().is_empty(),
            "the client secret must never leave the issuer origin, got {:?}",
            elsewhere.requested_paths()
        );
    }

    #[tokio::test]
    async fn client_credentials_exchange_refuses_same_origin_wrong_issuer_document() {
        // A co-tenant realm answering on the issuer's own origin passes the
        // path-blind origin pin, so only the issuer claim stops the secret
        // from reaching the wrong realm's token endpoint.
        let idp = IdpServer::start(|_, addr| {
            let origin = format!("http://127.0.0.1:{}", addr.port());

            http_json(&format!(
                "{{\"issuer\":\"{}/realms/other\",\"jwks_uri\":\"{}/jwks\",\
                 \"token_endpoint\":\"{}/realms/other/token\"}}",
                origin, origin, origin
            ))
        })
        .await;

        let err =
            exchange_client_credentials(&idp.issuer(), None, "vorpal-worker", "s3cr3t", "openid")
                .await
                .expect_err("a discovery document naming a different issuer must be refused");

        assert!(
            err.to_string().contains("issuer mismatch"),
            "unexpected error: {err}"
        );
        assert!(
            !idp.requested_paths()
                .iter()
                .any(|path| path.contains("token")),
            "the client secret must not be sent to a wrong-issuer endpoint, got {:?}",
            idp.requested_paths()
        );
    }

    #[tokio::test]
    async fn client_credentials_exchange_refuses_discovery_without_an_issuer_claim() {
        // Fails closed: an absent claim must not silently skip the compare.
        let idp = IdpServer::start(|_, addr| {
            let origin = format!("http://127.0.0.1:{}", addr.port());

            http_json(&format!("{{\"token_endpoint\":\"{}/token\"}}", origin))
        })
        .await;

        let err =
            exchange_client_credentials(&idp.issuer(), None, "vorpal-worker", "s3cr3t", "openid")
                .await
                .expect_err("a discovery document with no issuer claim must be refused");

        assert!(
            err.to_string().contains("failed to parse OIDC discovery"),
            "unexpected error: {err}"
        );
        assert!(
            !idp.requested_paths()
                .iter()
                .any(|path| path.contains("token")),
            "the client secret must not be sent without an issuer claim, got {:?}",
            idp.requested_paths()
        );
    }

    #[tokio::test]
    async fn client_credentials_exchange_accepts_a_trailing_slash_issuer_claim() {
        // Discriminates the normalized compare from a raw `==`: a correct IdP
        // may state its `iss` with a trailing slash the configured value lacks.
        let idp = IdpServer::start(|path, addr| {
            let origin = format!("http://127.0.0.1:{}", addr.port());

            match path {
                "/.well-known/openid-configuration" => http_json(&format!(
                    "{{\"issuer\":\"{}/\",\"jwks_uri\":\"{}/jwks\",\
                     \"token_endpoint\":\"{}/token\"}}",
                    origin, origin, origin
                )),
                _ => http_json(
                    "{\"access_token\":\"header.payload.signature\",\"expires_in\":300,\
                     \"token_type\":\"Bearer\"}",
                ),
            }
        })
        .await;

        let (header, _) =
            exchange_client_credentials(&idp.issuer(), None, "vorpal-worker", "s3cr3t", "openid")
                .await
                .expect("a trailing-slash issuer claim names the same issuer");

        assert_eq!(header.to_str().unwrap(), "Bearer header.payload.signature");
        assert!(idp.requested_paths().contains(&"/token".to_string()));
    }

    #[tokio::test]
    async fn client_credentials_exchange_refuses_redirected_token_endpoint() {
        // A 307 replays the request body, so following it would hand the
        // client secret to the redirect target.
        let elsewhere = IdpServer::start(|_, _| http_json("{}")).await;

        let target = format!("{}/token", elsewhere.issuer());
        let idp = IdpServer::start(move |path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            match path {
                "/.well-known/openid-configuration" => {
                    discovery_document(&issuer, &format!("{}/jwks", issuer))
                }
                _ => http_redirect("307 Temporary Redirect", &target),
            }
        })
        .await;

        let err =
            exchange_client_credentials(&idp.issuer(), None, "vorpal-worker", "s3cr3t", "openid")
                .await
                .expect_err("a redirected token endpoint must be refused");

        assert!(
            err.to_string().contains("refusing to follow redirect"),
            "unexpected error: {err}"
        );
        assert!(
            elsewhere.requested_paths().is_empty(),
            "the client secret must never be replayed at the redirect target, got {:?}",
            elsewhere.requested_paths()
        );
    }

    #[tokio::test]
    async fn client_credentials_exchange_does_not_relay_a_hostile_discovery_body_into_its_error() {
        let forged = format!(
            "not json\r\n\u{1b}[31mERROR worker |> forged record\u{1b}[0m\n{}",
            "A".repeat(RESPONSE_EXCERPT_LIMIT)
        );

        let idp = IdpServer::start(move |_, _| http_json(&forged)).await;

        let err =
            exchange_client_credentials(&idp.issuer(), None, "vorpal-worker", "s3cr3t", "openid")
                .await
                .expect_err("an unparseable discovery document must be refused");

        let rendered = err.to_string();

        assert!(
            !rendered.contains('\n') && !rendered.contains('\r') && !rendered.contains('\u{1b}'),
            "control characters reached the error text: {rendered:?}"
        );
        assert!(
            rendered.contains(RESPONSE_EXCERPT_TRUNCATION_MARKER),
            "an over-long body must be visibly truncated: {rendered:?}"
        );
        assert!(
            !rendered.contains(&"A".repeat(RESPONSE_EXCERPT_LIMIT)),
            "the full body must not reach the error text: {rendered:?}"
        );
    }

    #[test]
    fn response_excerpt_strips_control_characters_and_truncates() {
        let excerpt = response_excerpt(&format!(
            "line one\r\n\u{1b}[31mforged\u{1b}[0m{}",
            "z".repeat(RESPONSE_EXCERPT_LIMIT)
        ));

        assert!(!excerpt.contains('\n'));
        assert!(!excerpt.contains('\r'));
        assert!(!excerpt.contains('\u{1b}'));
        assert!(excerpt.ends_with(RESPONSE_EXCERPT_TRUNCATION_MARKER));
        assert_eq!(
            excerpt.chars().count(),
            RESPONSE_EXCERPT_LIMIT + RESPONSE_EXCERPT_TRUNCATION_MARKER.chars().count()
        );
    }

    #[test]
    fn response_excerpt_truncates_on_character_boundaries() {
        // A byte-index slice through a multi-byte character panics; this body
        // puts one exactly at the cap.
        let excerpt = response_excerpt(&"é".repeat(RESPONSE_EXCERPT_LIMIT + 5));

        assert_eq!(
            excerpt,
            format!(
                "{}{}",
                "é".repeat(RESPONSE_EXCERPT_LIMIT),
                RESPONSE_EXCERPT_TRUNCATION_MARKER
            )
        );
    }

    #[test]
    fn response_excerpt_passes_a_short_plain_body_through_unchanged() {
        // An operator debugging a real IdP error must still see the message.
        let body = "{\"error\":\"invalid_client\"}";

        assert_eq!(response_excerpt(body), body);
    }

    /// Build a validator against a loopback IdP serving an empty JWKS: enough
    /// for the interceptor to be constructed, and enough that any presented
    /// token fails verification rather than being accepted.
    async fn interceptor_over_empty_jwks_idp(
    ) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
        let idp = IdpServer::start(|path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            match path {
                "/.well-known/openid-configuration" => {
                    discovery_document(&issuer, &format!("{}/jwks", issuer))
                }
                _ => http_json(EMPTY_JWKS),
            }
        })
        .await;

        let validator = OidcValidator::new(idp.issuer(), vec![])
            .await
            .expect("loopback discovery must succeed");

        new_interceptor(Arc::new(validator))
    }

    // VPL-714 AC 1: the interceptor `start.rs` attaches to the archive and
    // artifact services is the first of the two links that keep a claim-free
    // request away from a registry handler, and nothing pinned it. It must
    // reject a request carrying no `authorization` metadata outright, so
    // `InterceptedService` never calls the inner service.
    #[tokio::test(flavor = "multi_thread")]
    async fn interceptor_denies_a_request_with_no_authorization_metadata() {
        let interceptor = interceptor_over_empty_jwks_idp().await;

        let status = interceptor(Request::new(()))
            .err()
            .expect("a request with no authorization metadata must be refused");

        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert_eq!(status.message(), "missing authorization");
    }

    // Known-positive control for the test above: it distinguishes "the
    // metadata gate fired" from "the fixture is inert". A request that does
    // carry an `authorization` header reaches token validation and fails
    // there instead, with a different message.
    #[tokio::test(flavor = "multi_thread")]
    async fn interceptor_rejects_a_presented_token_at_validation_not_at_the_metadata_gate() {
        let interceptor = interceptor_over_empty_jwks_idp().await;

        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            "Bearer not-a-real-token".parse().expect("header value"),
        );

        let status = interceptor(request)
            .err()
            .expect("a token that does not verify must be refused");

        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(
            status.message().starts_with("token invalid"),
            "the request must reach validation, got: {}",
            status.message()
        );
    }

    // ===== Cache-miss JWKS refresh floor (VPL-1403) =====
    //
    // The observable throughout is the fixture's recorded `/jwks` request
    // count, never the returned error: every miss below returns the same
    // `KeyNotFound` with and without the bound, so an assertion on the error
    // would prove nothing.

    /// A syntactically valid JWT whose header names `kid` and whose signature
    /// is garbage — everything an unauthenticated peer can produce for free,
    /// and all it takes to reach the refresh path.
    fn unsigned_token_with_kid(kid: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let header = URL_SAFE_NO_PAD.encode(format!(
            "{{\"alg\":\"RS256\",\"typ\":\"JWT\",\"kid\":\"{kid}\"}}"
        ));
        let payload = URL_SAFE_NO_PAD.encode("{\"sub\":\"nobody\"}");

        format!("Bearer {header}.{payload}.{}", URL_SAFE_NO_PAD.encode("x"))
    }

    /// A JWKS containing one RSA key under `kid`, with a well-formed but
    /// meaningless modulus: enough for `try_decode_with_kid` to select it and
    /// fail at the signature rather than at key lookup.
    fn jwks_with_kid(kid: &str) -> String {
        format!(
            "{{\"keys\":[{{\"kid\":\"{kid}\",\"kty\":\"RSA\",\"n\":\"{}\",\"e\":\"AQAB\"}}]}}",
            "sQ".repeat(128)
        )
    }

    /// An IdP whose `/jwks` body is swappable, so a key rotation can be staged
    /// mid-test, and whose served paths are counted by `IdpServer`.
    async fn rotatable_idp(initial_jwks: String) -> (IdpServer, Arc<std::sync::Mutex<String>>) {
        let served = Arc::new(std::sync::Mutex::new(initial_jwks));
        let jwks = served.clone();

        let idp = IdpServer::start(move |path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());

            match path {
                "/.well-known/openid-configuration" => {
                    discovery_document(&issuer, &format!("{}/jwks", issuer))
                }
                _ => http_json(&jwks.lock().unwrap().clone()),
            }
        })
        .await;

        (idp, served)
    }

    fn jwks_requests(idp: &IdpServer) -> usize {
        idp.requested_paths()
            .iter()
            .filter(|path| *path == "/jwks")
            .count()
    }

    // VPL-1403 AC 1 & 2: an unauthenticated peer varying `kid` must not buy one
    // outbound fetch per token. Mutant check: drop the elapsed-interval test in
    // `refresh_jwks_if_interval_elapsed` and this observes 8 misses, not 1.
    #[tokio::test]
    async fn cache_miss_refreshes_are_bounded_by_the_minimum_interval() {
        let (idp, _served) = rotatable_idp(EMPTY_JWKS.to_string()).await;

        let validator = OidcValidator::new(idp.issuer(), vec![])
            .await
            .expect("loopback discovery must succeed")
            .with_min_refresh_interval(Duration::from_secs(3600));

        let fetches_after_construction = jwks_requests(&idp);

        for attempt in 0..8 {
            let error = validator
                .validate(&unsigned_token_with_kid(&format!("unknown-{attempt}")))
                .await
                .err()
                .expect("an unknown kid must never validate");

            assert!(
                matches!(error, AuthError::KeyNotFound),
                "attempt {attempt} must miss, got: {error}"
            );
        }

        assert_eq!(
            jwks_requests(&idp) - fetches_after_construction,
            1,
            "8 distinct unknown kids inside one interval must buy one refresh, \
             served: {:?}",
            idp.requested_paths()
        );
    }

    // Benign control for the test above: suppression must not cost the fast
    // path. A `kid` that is already cached validates far enough to fail on its
    // signature, with no outbound request at all.
    #[tokio::test]
    async fn a_cached_kid_is_served_without_any_outbound_request() {
        let (idp, _served) = rotatable_idp(jwks_with_kid("cached")).await;

        let validator = OidcValidator::new(idp.issuer(), vec![])
            .await
            .expect("loopback discovery must succeed")
            .with_min_refresh_interval(Duration::from_secs(3600));

        let fetches_after_construction = jwks_requests(&idp);

        let error = validator
            .validate(&unsigned_token_with_kid("cached"))
            .await
            .err()
            .expect("a garbage signature must not validate");

        assert!(
            matches!(error, AuthError::Jwt(_)),
            "a cached kid must reach signature verification, got: {error}"
        );
        assert_eq!(
            jwks_requests(&idp),
            fetches_after_construction,
            "a cache hit must issue no refresh, served: {:?}",
            idp.requested_paths()
        );
    }

    // VPL-1403 AC 2 positive control: the bound delays key discovery by at most
    // one interval and never prevents it, so a genuine rotation is still picked
    // up. "Picked up" is observed as the rotated `kid` ceasing to be
    // `KeyNotFound` — it now selects a key and fails at the signature instead.
    #[tokio::test]
    async fn a_genuine_rotation_is_picked_up_once_the_interval_elapses() {
        let interval = Duration::from_millis(200);
        let (idp, served) = rotatable_idp(jwks_with_kid("before-rotation")).await;

        let validator = OidcValidator::new(idp.issuer(), vec![])
            .await
            .expect("loopback discovery must succeed")
            .with_min_refresh_interval(interval);

        // Spend the budget, so the rotation below lands inside the interval.
        let _ = validator.validate(&unsigned_token_with_kid("warmup")).await;

        *served.lock().unwrap() = jwks_with_kid("after-rotation");

        let suppressed = jwks_requests(&idp);
        let during_interval = validator
            .validate(&unsigned_token_with_kid("after-rotation"))
            .await
            .err()
            .expect("the rotated key is not cached yet");

        assert!(
            matches!(during_interval, AuthError::KeyNotFound),
            "inside the interval the rotated kid must still miss, got: {during_interval}"
        );
        assert_eq!(
            jwks_requests(&idp),
            suppressed,
            "inside the interval no fetch may be issued, served: {:?}",
            idp.requested_paths()
        );

        tokio::time::sleep(interval + Duration::from_millis(50)).await;

        let after_interval = validator
            .validate(&unsigned_token_with_kid("after-rotation"))
            .await
            .err()
            .expect("a garbage signature must not validate");

        assert!(
            matches!(after_interval, AuthError::Jwt(_)),
            "once the interval elapses the rotated key must be found, got: {after_interval}"
        );
        assert_eq!(
            jwks_requests(&idp),
            suppressed + 1,
            "the rotation must cost exactly one refresh, served: {:?}",
            idp.requested_paths()
        );
    }

    // VPL-1403: the interval test and the fetch it authorizes are one atomic
    // transition, so a concurrent flood cannot multiply the single refresh the
    // bound allows. Mutant check: release the lock before fetching and this
    // observes 8.
    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_cache_misses_issue_a_single_refresh() {
        let (idp, _served) = rotatable_idp(EMPTY_JWKS.to_string()).await;

        let validator = Arc::new(
            OidcValidator::new(idp.issuer(), vec![])
                .await
                .expect("loopback discovery must succeed")
                .with_min_refresh_interval(Duration::from_secs(3600)),
        );

        let fetches_after_construction = jwks_requests(&idp);

        let mut misses = tokio::task::JoinSet::new();

        for attempt in 0..8 {
            let validator = validator.clone();

            misses.spawn(async move {
                validator
                    .validate(&unsigned_token_with_kid(&format!("concurrent-{attempt}")))
                    .await
                    .err()
                    .expect("an unknown kid must never validate");
            });
        }

        misses.join_all().await;

        assert_eq!(
            jwks_requests(&idp) - fetches_after_construction,
            1,
            "8 concurrent misses must coalesce into one refresh, served: {:?}",
            idp.requested_paths()
        );
    }
}
