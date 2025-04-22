use crate::command::start::registry::{s3::get_artifact_key, ArtifactBackend, S3Backend};
use sha256::digest;
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::Artifact;

#[async_trait]
impl ArtifactBackend for S3Backend {
    async fn get_artifact(&self, artifact_digest: String) -> Result<Artifact, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_key = get_artifact_key(&artifact_digest);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(&artifact_key)
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        let mut artifact_stream = client
            .get_object()
            .bucket(bucket)
            .key(&artifact_key)
            .send()
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .body;

        let mut artifact_json = String::new();

        while let Some(chunk) = artifact_stream.next().await {
            let artifact_chunk = chunk.map_err(|err| Status::internal(err.to_string()))?;

            artifact_json.push_str(&String::from_utf8_lossy(&artifact_chunk));
        }

        let artifact: Artifact = serde_json::from_str(&artifact_json)
            .map_err(|err| Status::internal(format!("failed to parse artifact: {:?}", err)))?;

        Ok(artifact)
    }

    async fn store_artifact(&self, request: &Artifact) -> Result<String, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_json = serde_json::to_vec(request)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let artifact_digest = digest(&artifact_json);
        let artifact_key = get_artifact_key(&artifact_digest);

        let artifact_head = client
            .head_object()
            .bucket(bucket)
            .key(&artifact_key)
            .send()
            .await;

        if artifact_head.is_ok() {
            return Ok(artifact_digest);
        }

        let artifact_json = serde_json::to_vec(request)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;

        client
            .put_object()
            .bucket(bucket)
            .key(artifact_key)
            .body(artifact_json.into())
            .send()
            .await
            .map_err(|err| Status::internal(format!("failed to write store config: {:?}", err)))?;

        Ok(artifact_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
