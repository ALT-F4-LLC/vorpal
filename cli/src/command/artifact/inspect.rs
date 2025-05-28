use anyhow::Result;
use tracing::{subscriber, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};
use vorpal_sdk::api::artifact::{artifact_service_client::ArtifactServiceClient, ArtifactRequest};

pub async fn run(digest: &str, level: Level, registry: &str) -> Result<()> {
    let subscriber_writer = std::io::stderr.with_max_level(level);

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_writer(subscriber_writer)
        .without_time()
        .finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber");

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

    println!("{}", artifact_data);

    Ok(())
}
