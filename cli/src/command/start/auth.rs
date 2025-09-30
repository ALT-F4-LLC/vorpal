use crate::command::store::paths::get_key_service_secret_path;
use anyhow::Result;
use std::env::var;
use tokio::fs::read_to_string;
use tonic::{Request, Status};

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

pub fn create_interceptor(
    service_token: String,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> + Clone {
    move |request: Request<()>| -> Result<Request<()>, Status> {
        match request.metadata().get("authorization") {
            None => Err(Status::unauthenticated("Missing authorization header")),

            Some(t) => {
                let authorization = t.to_str().unwrap_or("").trim();
                let authorization_token = &authorization[7..];

                if authorization_token == service_token {
                    Ok(request)
                } else {
                    Err(Status::unauthenticated("Invalid token"))
                }
            }
        }
    }
}

/// Loads the user API token from VORPAL_API_TOKEN environment variable
/// Used as a fallback when no API token is provided by CLI commands
pub fn load_api_token_env() -> Result<String> {
    match var("VORPAL_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => Ok(token.trim().to_string()),
        _ => Err(anyhow::anyhow!(
            "No API token found. Please set VORPAL_API_TOKEN environment variable or add 'api_token' to Vorpal.toml"
        )),
    }
}
