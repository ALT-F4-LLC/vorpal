use anyhow::{anyhow, Result};
use tonic::Request;
use vorpal_sdk::{
    api::artifact::{artifact_service_client::ArtifactServiceClient, ArtifactRequest},
    context::{build_channel, client_auth_header},
};

pub async fn run(digest: &str, namespace: &str, registry: &str) -> Result<()> {
    let client_channel = build_channel(registry).await?;
    let mut client = ArtifactServiceClient::new(client_channel);

    // Create request

    let request = ArtifactRequest {
        digest: digest.to_string(),
        namespace: namespace.to_string(),
    };

    let mut request = Request::new(request);
    let request_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {}", e))?;

    if let Some(header) = request_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    let artifact_response = client.get_artifact(request).await?;
    let artifact = artifact_response.into_inner();
    let artifact_data = serde_json::to_string_pretty(&artifact)?;

    println!("{artifact_data}");

    Ok(())
}
