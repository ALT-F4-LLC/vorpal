use crate::command::store::paths::get_key_ca_path;
use anyhow::Result;
use http::uri::Uri;
use tokio::fs::read;
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig},
    Request,
};
use vorpal_sdk::api::artifact::{artifact_service_client::ArtifactServiceClient, ArtifactRequest};

pub async fn run(digest: &str, namespace: &str, registry: &str) -> Result<()> {
    // Setup TLS with CA certificate

    let client_ca_pem_path = get_key_ca_path();
    let client_ca_pem = read(client_ca_pem_path).await?;
    let client_ca = Certificate::from_pem(client_ca_pem);

    let client_tls = ClientTlsConfig::new()
        .ca_certificate(client_ca)
        .domain_name("localhost");

    // Parse registry URI and create authenticated channel

    let client_uri = registry.parse::<Uri>()?;

    let client_channel = Channel::builder(client_uri)
        .tls_config(client_tls)?
        .connect()
        .await?;

    let mut client = ArtifactServiceClient::new(client_channel);

    // Create request

    let request = ArtifactRequest {
        digest: digest.to_string(),
        namespace: namespace.to_string(),
    };

    let request = Request::new(request);

    let artifact_response = client.get_artifact(request).await?;
    let artifact = artifact_response.into_inner();
    let artifact_data = serde_json::to_string_pretty(&artifact)?;

    println!("{artifact_data}");

    Ok(())
}
