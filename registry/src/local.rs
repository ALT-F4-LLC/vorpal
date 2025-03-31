use crate::{RegistryBackend, RegistryError, DEFAULT_GRPC_CHUNK_SIZE};
use sha256::digest;
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use vorpal_schema::{
    config::v0::{ConfigArtifact, ConfigArtifactRequest},
    registry::v0::{RegistryPullRequest, RegistryPullResponse, RegistryPushRequest},
};
use vorpal_store::paths::{get_archive_path, get_store_config_path, set_timestamps};

#[derive(Clone, Debug)]
pub struct LocalRegistryBackend;

impl LocalRegistryBackend {
    pub fn new() -> Result<Self, RegistryError> {
        Ok(Self)
    }
}

#[async_trait]
impl RegistryBackend for LocalRegistryBackend {
    async fn get_archive(&self, request: &RegistryPullRequest) -> Result<(), Status> {
        let artifact_path = get_archive_path(&request.hash);

        if !artifact_path.exists() {
            return Err(Status::not_found("store config not found"));
        }

        Ok(())
    }

    async fn get_artifact(
        &self,
        request: &ConfigArtifactRequest,
    ) -> Result<ConfigArtifact, Status> {
        let artifact_path = get_store_config_path(&request.hash);

        if !artifact_path.exists() {
            return Err(Status::not_found("store config not found"));
        }

        let artifact_data = read(&artifact_path)
            .await
            .map_err(|err| Status::internal(format!("failed to read config: {:?}", err)))?;

        let artifact: ConfigArtifact = serde_json::from_slice(&artifact_data)
            .map_err(|err| Status::internal(format!("failed to parse config: {:?}", err)))?;

        Ok(artifact)
    }

    async fn pull_archive(
        &self,
        request: &RegistryPullRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status> {
        let archive_path = get_archive_path(&request.hash);

        if !archive_path.exists() {
            return Err(Status::not_found("store path not found"));
        }

        let archive_data = read(&archive_path)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        for chunk in archive_data.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
            tx.send(Ok(RegistryPullResponse {
                data: chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        Ok(())
    }

    async fn push_archive(&self, request: &RegistryPushRequest) -> Result<(), Status> {
        let archive_path = get_archive_path(&request.hash);

        if !archive_path.exists() {
            write(&archive_path, &request.data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;

            set_timestamps(&archive_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;
        }

        Ok(())
    }

    async fn put_artifact(&self, request: &ConfigArtifact) -> Result<(), Status> {
        let artifact_json = serde_json::to_vec(request)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let artifact_hash = digest(&artifact_json);
        let artifact_path = get_store_config_path(&artifact_hash);

        if !artifact_path.exists() {
            write(&artifact_path, serde_json::to_vec(request).unwrap())
                .await
                .map_err(|err| {
                    Status::internal(format!("failed to write store config: {:?}", err))
                })?;

            set_timestamps(&artifact_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;
        }

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn RegistryBackend> {
        Box::new(self.clone())
    }
}
