use anyhow::Result;
use vorpal_sdk::api::artifact::{artifact_service_client::ArtifactServiceClient, ArtifactRequest};

pub async fn run(digest: &str, registry: &str) -> Result<()> {
    let mut client = ArtifactServiceClient::connect(registry.to_owned())
        .await
        .expect("failed to connect to registry");

    let request = ArtifactRequest {
        digest: digest.to_string(),
    };

    let response = client
        .get_artifact(request)
        .await
        .expect("failed to get artifact");

    let artifact = response.into_inner();

    let artifact_data =
        serde_json::to_string_pretty(&artifact).expect("failed to serialize artifact");

    println!("{artifact_data}");

    Ok(())
}
