use anyhow::Result;
use http::uri::Uri;
use tonic::{transport::Channel, Request};
use vorpal_sdk::{
    api::artifact::{artifact_service_client::ArtifactServiceClient, ArtifactRequest},
    context::get_client_tls_config,
};

pub async fn run(digest: &str, namespace: &str, registry: &str) -> Result<()> {
    // Setup TLS with CA certificate

    let client_tls = get_client_tls_config().await?;

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
