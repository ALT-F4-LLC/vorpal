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
    #[allow(dead_code)]
    pub azp: Option<String>,
    #[allow(dead_code)]
    pub gty: Option<String>,

    // Namespace permissions
    pub namespaces: Option<HashMap<String, Vec<String>>>,
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
        })
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
        let validator = validator.clone();
        let claims = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current()
                .block_on(async move { validator.validate(&auth).await })
        })
        .map_err(|e| Status::unauthenticated(format!("token invalid: {e}")))?;

        // Stash claims for handlers
        req.extensions_mut().insert(claims);
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
        scope: scope.to_string(),
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

/// Extract user context for audit logging
pub fn get_user_context<T>(request: &Request<T>) -> Option<String> {
    request
        .extensions()
        .get::<Claims>()
        .and_then(|claims| claims.subject().map(String::from))
}
