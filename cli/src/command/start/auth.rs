use anyhow::{anyhow, Context, Result};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tonic::{
    metadata::{Ascii, MetadataValue},
    Request, Status,
};
use tracing::error;

#[derive(Debug, Deserialize, Clone)]
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
    #[allow(dead_code)]
    pub aud: Option<Value>,
    #[allow(dead_code)]
    pub exp: Option<u64>,
    #[allow(dead_code)]
    pub iss: Option<String>,
    pub sub: Option<String>,
    #[allow(dead_code)]
    pub scope: Option<String>,
    pub azp: Option<String>,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
}

impl OidcValidator {
    pub async fn new(issuer: String, issuer_audiences: Vec<String>) -> Result<Self> {
        // Normalize issuer by removing trailing slash for consistent comparison
        let normalized_issuer = issuer.trim_end_matches('/').to_string();

        // 1) Discover the realm (/.well-known/openid-configuration)
        let discovery_url = format!("{}/.well-known/openid-configuration", normalized_issuer);
        let disc: OidcDiscovery = reqwest::Client::new()
            .get(&discovery_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("parsing discovery doc")?;

        // Normalize discovery issuer for comparison (Auth0 always includes trailing slash)
        let normalized_disc_issuer = disc.issuer.trim_end_matches('/').to_string();

        if normalized_disc_issuer != normalized_issuer {
            // Defensive: enforce exact issuer match
            return Err(anyhow!(
                "issuer mismatch (expected {}, discovery says {})",
                normalized_issuer,
                disc.issuer
            ));
        }

        // 2) Fetch JWKS
        let jwks = fetch_jwks(&disc.jwks_uri).await?;

        Ok(Self {
            issuer: normalized_issuer,
            issuer_audiences,
            jwks: Arc::new(RwLock::new(jwks)),
            jwks_uri: disc.jwks_uri,
            trusted_service_client_ids: Vec::new(),
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

    async fn validate(&self, bearer: &str) -> Result<Claims, AuthError> {
        let token = bearer
            .strip_prefix("Bearer ")
            .ok_or(AuthError::InvalidScheme)?;

        // Decode header to pick the right key (kid)
        let header = decode_header(token).map_err(|e| AuthError::Jwt(e.to_string()))?;
        let kid = header.kid.ok_or(AuthError::MissingKid)?;
        let aud: Vec<&str> = self.issuer_audiences.iter().map(|s| s.as_str()).collect();

        // Try current cache
        if let Some(claims) = self.try_decode_with_kid(aud.clone(), &kid, token).await? {
            // self.validate_claims(&claims)?;
            return Ok(claims);
        }

        // If not found, refresh JWKS once and retry (handles rolling keys)
        let fresh = fetch_jwks(&self.jwks_uri)
            .await
            .map_err(|e| AuthError::Jwt(format!("jwks refresh failed: {e}")))?;
        *self.jwks.write().await = fresh;

        if let Some(claims) = self.try_decode_with_kid(aud, &kid, token).await? {
            // self.validate_claims(&claims)?;
            return Ok(claims);
        }

        Err(AuthError::KeyNotFound)
    }

    async fn try_decode_with_kid(
        &self,
        aud: Vec<&str>,
        kid: &str,
        token: &str,
    ) -> Result<Option<Claims>, AuthError> {
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
            .clone()
            .ok_or_else(|| AuthError::Jwt("missing n".into()))?;
        let e = jwk
            .e
            .clone()
            .ok_or_else(|| AuthError::Jwt("missing e".into()))?;

        let key =
            DecodingKey::from_rsa_components(&n, &e).map_err(|e| AuthError::Jwt(e.to_string()))?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&aud);
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

async fn fetch_jwks(uri: &str) -> Result<JwkSet> {
    let jwks: JwkSet = reqwest::Client::new()
        .get(uri)
        .send()
        .await?
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
        let validator_for_validate = validator.clone();
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

#[derive(Debug, Deserialize, Clone)]
struct TokenEndpointDiscovery {
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
    #[allow(dead_code)]
    token_type: String,
}

/// Compose the OAuth2 scope string for a client_credentials request.
///
/// When `issuer_audience` is provided, appends Zitadel's project-audience-injection
/// scope (`urn:zitadel:iam:org:project:id:{aud}:aud`). Zitadel v4 silently ignores
/// the `audience` form-body parameter for the client-credentials grant — the URN
/// scope is the only way to force the project ID into the minted token's `aud`.
/// Keycloak and Auth0 ignore unrecognized scope strings, so the appended URN is
/// harmless against them.
fn compose_client_credentials_scope(scope: &str, issuer_audience: Option<&str>) -> String {
    match issuer_audience {
        Some(aud) => format!("{} urn:zitadel:iam:org:project:id:{}:aud", scope, aud),
        None => scope.to_string(),
    }
}

/// Performs OAuth2 Client Credentials Flow token exchange for service-to-service authentication
///
/// Args:
/// - issuer: Base URL of the OIDC provider (e.g., http://localhost:8080/realms/vorpal)
/// - client_id: Service account client ID
/// - client_secret: Service account client secret
/// - scope: OAuth2 scope to request
/// - audience: API identifier
///
/// Returns: Bearer token as MetadataValue and expires_in suitable for gRPC requests
pub async fn exchange_client_credentials(
    issuer: &str,
    issuer_audience: Option<&str>,
    issuer_client_id: &str,
    issuer_client_secret: &str,
    scope: &str,
) -> Result<(MetadataValue<Ascii>, u64)> {
    // 1) Discover the token endpoint via OIDC discovery
    let discovery_url = format!("{}/.well-known/openid-configuration", issuer);

    let discovery_response = reqwest::Client::new()
        .get(&discovery_url)
        .send()
        .await
        .context("failed to fetch OIDC discovery")?;

    // let discovery_status = discovery_response.status();
    let discovery_text = discovery_response
        .text()
        .await
        .unwrap_or_else(|_| "<unable to read response body>".to_string());

    let disc: TokenEndpointDiscovery = serde_json::from_str(&discovery_text).map_err(|e| {
        error!(
            "auth |> failed to parse OIDC discovery response: {} - full response: {}",
            e, discovery_text
        );
        anyhow!(
            "failed to parse OIDC discovery response: {} - response was: {}",
            e,
            discovery_text
        )
    })?;

    // 2) Exchange client credentials for access token
    let token_request = ClientCredentialsRequest {
        audience: issuer_audience.map(|s| s.to_string()),
        client_id: issuer_client_id.to_string(),
        client_secret: issuer_client_secret.to_string(),
        grant_type: "client_credentials".to_string(),
        scope: compose_client_credentials_scope(scope, issuer_audience),
    };

    let token_response = reqwest::Client::new()
        .post(&disc.token_endpoint)
        .form(&token_request)
        .send()
        .await
        .context("failed to send token request to OIDC provider")?;

    let token_status = token_response.status();
    let token_text = token_response
        .text()
        .await
        .unwrap_or_else(|_| "<unable to read response body>".to_string());

    if !token_status.is_success() {
        error!(
            "auth |> token exchange failed with status {}: {}",
            token_status, token_text
        );
        return Err(anyhow!(
            "token endpoint returned {}: {}",
            token_status,
            token_text
        ));
    }

    let response: ClientCredentialsResponse = serde_json::from_str(&token_text).map_err(|e| {
        error!(
            "auth |> failed to parse token response: {} - full response: {}",
            e, token_text
        );
        anyhow!(
            "failed to parse token response: {} - response was: {}",
            e,
            token_text
        )
    })?;

    // 3) Create Bearer token header
    let auth_header: MetadataValue<Ascii> = format!("Bearer {}", response.access_token)
        .parse()
        .map_err(|e| anyhow!("failed to parse Bearer token: {}", e))?;

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
            "insufficient permissions: no {} access to namespace: {}",
            permission, namespace
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
    fn require_namespace_or_service_trust_human_without_permission_fails() {
        let mut req = Request::new(());
        req.extensions_mut().insert(mk_claims(Some(HashMap::new())));
        req.extensions_mut().insert(PrincipalKind::Human);

        let err = require_namespace_or_service_trust(&req, "library", "read")
            .expect_err("should be denied");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        // TDD §6 error taxonomy: the wire-format message must be EXACTLY
        // "insufficient permissions: no {perm} access to namespace: {ns}".
        // External tooling and DKT-67 scenario 4 (unknown azp) assert against
        // this string — a substring check would mask drift. Keep verbatim.
        assert_eq!(
            err.message(),
            "insufficient permissions: no read access to namespace: library"
        );
    }

    #[test]
    fn require_namespace_or_service_trust_human_partial_permission_other_namespace_fails() {
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

        let err = require_namespace_or_service_trust(&req, "other", "write")
            .expect_err("should be denied");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no write access to namespace: other"
        );
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
    fn require_namespace_or_service_trust_no_principal_fails_unauthenticated() {
        let req: Request<()> = Request::new(());

        let err = require_namespace_or_service_trust(&req, "library", "read")
            .expect_err("should be unauthenticated");
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
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
        assert!(result.is_ok(), "trusted service must bypass: {:?}", result);
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
            "human with `library:write` must succeed in `library`: {:?}",
            result
        );
    }

    #[test]
    fn dkt67_scenario2_keycloak_regression_human_with_namespaces_fails_in_unowned_ns() {
        // Companion: same human, build in `other` → PERMISSION_DENIED with the
        // exact TDD §6 error string.
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["write".to_string()]);
        let claims = mk_claims_with_azp(Some("cli"), Some(ns));
        let req = build_request_with_claims_and_classification(claims, &[]);

        let err = require_namespace_or_service_trust(&req, "other", "write")
            .expect_err("human without `other:write` must be denied");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no write access to namespace: other"
        );
    }

    #[test]
    fn dkt67_scenario4_unknown_azp_returns_permission_denied_with_exact_error_string() {
        // Negative — token from a service account whose `azp` is NOT in the
        // allow-list. The flag is set (worker-client trusted), but this token's
        // `azp` is `attacker-client`, which falls back to the Human path.
        // Without `namespaces`, the gate denies with the EXACT TDD §6 string.
        let claims = mk_claims_with_azp(Some("attacker-client"), None);
        let req =
            build_request_with_claims_and_classification(claims, &["worker-client".to_string()]);

        let err = require_namespace_or_service_trust(&req, "library", "read")
            .expect_err("unknown azp must be denied");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no read access to namespace: library"
        );
    }

    #[test]
    fn dkt67_scenario4_unknown_azp_with_partial_namespaces_still_namespace_gated() {
        // Variant: unknown azp + a `namespaces` claim that doesn't cover the
        // requested ns → still PERMISSION_DENIED. Confirms the bypass is NOT
        // triggered just because the token has an `azp` field; the allow-list
        // membership is the ONLY classifier into TrustedService.
        let mut ns = HashMap::new();
        ns.insert("library".to_string(), vec!["read".to_string()]);
        let claims = mk_claims_with_azp(Some("attacker-client"), Some(ns));
        let req =
            build_request_with_claims_and_classification(claims, &["worker-client".to_string()]);

        let err = require_namespace_or_service_trust(&req, "library", "write")
            .expect_err("unknown azp without `library:write` must be denied");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
        assert_eq!(
            err.message(),
            "insufficient permissions: no write access to namespace: library"
        );
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
            "trusted azp must bypass even with no namespaces claim: {:?}",
            result
        );
    }
}
