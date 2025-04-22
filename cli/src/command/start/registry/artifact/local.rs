use crate::command::{
    start::registry::{ArtifactBackend, LocalBackend},
    store::paths::{get_config_path, set_timestamps},
};
use sha256::digest;
use tokio::fs::{read, write};
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::Artifact;

#[async_trait]
impl ArtifactBackend for LocalBackend {
    async fn get_artifact(&self, artifact_digest: String) -> Result<Artifact, Status> {
        let request_path = get_config_path(&artifact_digest);

        if !request_path.exists() {
            return Err(Status::not_found("store config not found"));
        }

        let artifact_data = read(&request_path)
            .await
            .map_err(|err| Status::internal(format!("failed to read config: {:?}", err)))?;

        let artifact: Artifact = serde_json::from_slice(&artifact_data)
            .map_err(|err| Status::internal(format!("failed to parse config: {:?}", err)))?;

        Ok(artifact)
    }

    async fn store_artifact(&self, request: &Artifact) -> Result<String, Status> {
        let request_json = serde_json::to_vec(request)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let request_digest = digest(&request_json);
        let request_path = get_config_path(&request_digest);

        if !request_path.exists() {
            write(&request_path, serde_json::to_vec(request).unwrap())
                .await
                .map_err(|err| {
                    Status::internal(format!("failed to write store config: {:?}", err))
                })?;

            set_timestamps(&request_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;
        }

        Ok(request_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
