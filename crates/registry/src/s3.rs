use crate::{RegistryBackend, RegistryError};
use aws_sdk_s3::Client;
use sha256::digest;
use tokio::sync::mpsc;
use tonic::{async_trait, Status};
use vorpal_schema::{
    config::v0::{ConfigArtifact, ConfigArtifactRequest},
    registry::v0::{RegistryPullRequest, RegistryPullResponse, RegistryPushRequest},
};

#[derive(Clone, Debug)]
pub struct S3RegistryBackend {
    bucket: String,
    client: Client,
}

impl S3RegistryBackend {
    pub async fn new(bucket: Option<String>) -> Result<Self, RegistryError> {
        let Some(bucket) = bucket else {
            return Err(RegistryError::MissingS3Bucket);
        };

        let client_config = aws_config::load_from_env().await;
        let client = Client::new(&client_config);

        Ok(Self { bucket, client })
    }
}

fn get_archive_key(hash: &str) -> String {
    format!("store/{hash}.tar.zst")
}

fn get_config_key(hash: &str) -> String {
    format!("store/{hash}.json")
}

#[async_trait]
impl RegistryBackend for S3RegistryBackend {
    async fn get_archive(&self, request: &RegistryPullRequest) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let archive_key = get_archive_key(&request.hash);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(archive_key.clone())
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        Ok(())
    }

    async fn get_artifact(
        &self,
        request: &ConfigArtifactRequest,
    ) -> Result<ConfigArtifact, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_key = get_config_key(&request.hash);

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

        let artifact: ConfigArtifact = serde_json::from_str(&artifact_json)
            .map_err(|err| Status::internal(format!("failed to parse artifact: {:?}", err)))?;

        Ok(artifact)
    }

    async fn pull_archive(
        &self,
        request: &RegistryPullRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let archive_key = get_archive_key(&request.hash);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(archive_key.clone())
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        let mut archive_stream = client
            .get_object()
            .bucket(bucket)
            .key(archive_key)
            .send()
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .body;

        while let Some(chunk) = archive_stream.next().await {
            let archive_chunk = chunk.map_err(|err| Status::internal(err.to_string()))?;

            tx.send(Ok(RegistryPullResponse {
                data: archive_chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        Ok(())
    }

    async fn push_archive(&self, request: &RegistryPushRequest) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let archive_key = get_archive_key(&request.hash);

        let archive_head = client
            .head_object()
            .bucket(bucket)
            .key(&archive_key)
            .send()
            .await;

        if archive_head.is_ok() {
            return Ok(());
        }

        client
            .put_object()
            .bucket(bucket)
            .key(archive_key)
            .body(request.data.clone().into())
            .send()
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        Ok(())
    }

    async fn put_artifact(&self, request: &ConfigArtifact) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_json = serde_json::to_vec(request)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let artifact_hash = digest(&artifact_json);
        let artifact_key = get_config_key(&artifact_hash);

        let artifact_head = client
            .head_object()
            .bucket(bucket)
            .key(&artifact_key)
            .send()
            .await;

        if artifact_head.is_ok() {
            return Ok(());
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

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn RegistryBackend> {
        Box::new(self.clone())
    }
}
