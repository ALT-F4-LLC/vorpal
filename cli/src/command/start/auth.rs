use crate::command::{credentials::VorpalCredentials, get_key_credentials_path};
use anyhow::{anyhow, Context, Result};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::sync::Arc;
use tokio::{fs::read, sync::RwLock};
use tonic::{
    metadata::{Ascii, MetadataValue},
    Request, Status,
};

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
    alg: Option<String>,
    n: Option<String>, // RSA modulus (base64url)
    e: Option<String>, // RSA exponent (base64url)
                       // (add EC/Ed25519 members if you enable those in Keycloak)
}

#[derive(Debug, Deserialize, Clone)]
struct Claims {
    // Standard-ish
    sub: Option<String>,
    iss: Option<String>,
    aud: Option<serde_json::Value>, // can be string or array
    exp: Option<u64>,
    nbf: Option<u64>,
    iat: Option<u64>,
    // Useful Keycloak extras
    scope: Option<String>,
    azp: Option<String>,
    preferred_username: Option<String>,
    email: Option<String>,
}

#[derive(Debug, thiserror::Error)]
enum AuthError {
    #[error("missing authorization header")]
    MissingAuthHeader,
    #[error("invalid authorization scheme")]
    InvalidScheme,
    #[error("token header missing kid")]
    MissingKid,
    #[error("no matching JWK for kid")]
    KeyNotFound,
    #[error("token validation failed: {0}")]
    Jwt(String),
    #[error("bad issuer")]
    Issuer,
    #[error("bad audience")]
    Audience,
}

pub struct OidcValidator {
    pub issuer: String,
    pub expected_aud: String,
    pub jwks_uri: String,
    // Cache the current JWK set; refresh if key not found
    jwks: Arc<RwLock<JwkSet>>,
}

impl OidcValidator {
    pub async fn new(issuer: String, expected_aud: String) -> Result<Self> {
        // 1) Discover the realm (/.well-known/openid-configuration)
        let discovery_url = format!("{}/.well-known/openid-configuration", issuer);
        let disc: OidcDiscovery = reqwest::Client::new()
            .get(&discovery_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("parsing discovery doc")?;

        if disc.issuer != issuer {
            // Defensive: enforce exact issuer match
            return Err(anyhow!("issuer mismatch (discovery says {})", disc.issuer));
        }

        // 2) Fetch JWKS
        let jwks = fetch_jwks(&disc.jwks_uri).await?;

        Ok(Self {
            issuer,
            expected_aud,
            jwks_uri: disc.jwks_uri,
            jwks: Arc::new(RwLock::new(jwks)),
        })
    }

    async fn validate(&self, bearer: &str) -> Result<Claims, AuthError> {
        let token = bearer
            .strip_prefix("Bearer ")
            .ok_or(AuthError::InvalidScheme)?;

        // Decode header to pick the right key (kid)
        let header = decode_header(token).map_err(|e| AuthError::Jwt(e.to_string()))?;
        let kid = header.kid.ok_or(AuthError::MissingKid)?;

        // Try current cache
        if let Some(claims) = self.try_decode_with_kid(token, &kid).await? {
            // self.validate_claims(&claims)?;
            return Ok(claims);
        }

        // If not found, refresh JWKS once and retry (handles rolling keys)
        let fresh = fetch_jwks(&self.jwks_uri)
            .await
            .map_err(|e| AuthError::Jwt(format!("jwks refresh failed: {e}")))?;
        *self.jwks.write().await = fresh;

        if let Some(claims) = self.try_decode_with_kid(token, &kid).await? {
            // self.validate_claims(&claims)?;
            return Ok(claims);
        }

        Err(AuthError::KeyNotFound)
    }

    async fn try_decode_with_kid(
        &self,
        token: &str,
        kid: &str,
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
        validation.set_audience(&[&self.expected_aud]);
        validation.set_issuer(&[&self.issuer]);
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

pub async fn client_auth_header(registry: &str) -> Result<Option<MetadataValue<Ascii>>> {
    let mut client_auth_header: Option<MetadataValue<_>> = None;

    let credentials_path = get_key_credentials_path();

    if credentials_path.exists() {
        let credentials_data = read(credentials_path)
            .await
            .map_err(|e| anyhow!("failed to read credentials file: {}", e))?;

        let credentials: VorpalCredentials = serde_json::from_slice(&credentials_data)
            .map_err(|e| anyhow!("failed to parse credentials file: {}", e))?;

        let registry_issuer = credentials
            .registry
            .get(registry)
            .ok_or_else(|| anyhow!("no credentials found for registry: {}", registry))?;

        let issuer_credentials = credentials.issuer.get(registry_issuer).ok_or_else(|| {
            anyhow!(
                "no issuer found for registry: {} (issuer: {})",
                registry,
                registry_issuer
            )
        })?;

        client_auth_header = Some(
            format!("Bearer {}", issuer_credentials.access_token)
                .parse()
                .map_err(|e| anyhow!("failed to parse service secret: {}", e))?,
        );
    }

    Ok(client_auth_header)
}
