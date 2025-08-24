use crate::command::store::paths::get_key_service_secret_path;
use anyhow::Result;
use std::env::var;
use tokio::fs::read_to_string;
use tonic::{metadata::MetadataValue, Request, Status};
use tracing::warn;

pub async fn load_service_secret() -> Result<String> {
    let secret_path = get_key_service_secret_path();

    if !secret_path.exists() {
        return Err(anyhow::anyhow!(
            "service secret not found - run 'vorpal system keys generate'"
        ));
    }

    let secret = read_to_string(secret_path).await?.trim().to_string();

    Ok(secret)
}

pub fn create_auth_interceptor(
    expected_token: String,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |req: Request<()>| -> Result<Request<()>, Status> {
        let token_header: MetadataValue<_> = expected_token.parse().unwrap();

        match req.metadata().get("authorization") {
            Some(t) if token_header == *t => Ok(req),
            Some(_) => {
                warn!("Invalid token provided");
                Err(Status::unauthenticated("Invalid token"))
            }
            None => {
                warn!("Missing authorization header");
                Err(Status::unauthenticated("Missing authorization header"))
            }
        }
    }
}

/// Loads the user API token from VORPAL_API_TOKEN environment variable
/// Used as a fallback when no API token is provided by CLI commands
pub fn load_user_api_token_from_env() -> Result<String> {
    match var("VORPAL_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => Ok(token.trim().to_string()),
        _ => Err(anyhow::anyhow!(
            "No API token found. Please set VORPAL_API_TOKEN environment variable or add 'api_token' to Vorpal.toml"
        )),
    }
}
