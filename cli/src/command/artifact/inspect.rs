use crate::command::{start::auth::load_user_api_token_from_env, store::paths::get_key_ca_path};
use anyhow::Result;
use http::uri::{InvalidUri, Uri};
use tokio::fs::read;
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig},
    Request,
};
use vorpal_sdk::api::artifact::{artifact_service_client::ArtifactServiceClient, ArtifactRequest};

pub async fn run(digest: &str, registry: &str, api_token: Option<String>) -> Result<()> {
    // Setup TLS with CA certificate
    let client_ca_pem_path = get_key_ca_path();
    let client_ca_pem = read(client_ca_pem_path).await?;
    let client_ca = Certificate::from_pem(client_ca_pem);

    let client_tls = ClientTlsConfig::new()
        .ca_certificate(client_ca)
        .domain_name("localhost");

    // Parse registry URI and create authenticated channel
    let client_registry_uri = registry
        .parse::<Uri>()
        .map_err(|e: InvalidUri| anyhow::anyhow!("invalid registry address: {}", e))?;

    let client_registry_channel = Channel::builder(client_registry_uri)
        .tls_config(client_tls)?
        .connect()
        .await?;

    let mut client = ArtifactServiceClient::new(client_registry_channel);

    // Use the provided API token, falling back to environment variable only
    let user_api_token = match api_token {
        Some(token) => token,
        None => load_user_api_token_from_env()?,
    };

    // Create authenticated request
    let request = ArtifactRequest {
        digest: digest.to_string(),
    };

    let mut grpc_request = Request::new(request);

    grpc_request.metadata_mut().insert(
        "authorization",
        user_api_token
            .parse()
            .expect("failed to set authorization header"),
    );

    let response = client
        .get_artifact(grpc_request)
        .await
        .expect("failed to get artifact");

    let artifact = response.into_inner();

    let artifact_data =
        serde_json::to_string_pretty(&artifact).expect("failed to serialize artifact");

    println!("{artifact_data}");

    Ok(())
}
