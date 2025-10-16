use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct VorpalCredentialsContent {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VorpalCredentials {
    pub issuer: HashMap<String, VorpalCredentialsContent>,
    pub registry: HashMap<String, String>,
}
