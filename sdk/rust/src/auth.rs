use anyhow::{bail, Result};
use std::{env, path::PathBuf};
use tokio::fs::read_to_string;

/// Returns the path to the service authentication secret
pub fn get_service_secret_path() -> PathBuf {
    PathBuf::from("/var/lib/vorpal/key/service.secret")
}

/// Loads the service authentication secret from the standard location
pub async fn load_service_secret() -> Result<String> {
    let secret_path = get_service_secret_path();

    if !secret_path.exists() {
        return Err(anyhow::anyhow!(
            "service secret not found - run 'vorpal system keys generate'"
        ));
    }

    let secret = read_to_string(secret_path).await?.trim().to_string();

    Ok(secret)
}

/// Loads the user API token from VORPAL_API_TOKEN environment variable
pub fn load_user_api_token() -> Result<String> {
    match env::var("VORPAL_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => Ok(token.trim().to_string()),
        _ => bail!("VORPAL_API_TOKEN environment variable not set or empty"),
    }
}
