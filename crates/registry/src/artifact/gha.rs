use crate::{
    artifact::ArtifactBackend,
    gha::{get_artifact_key, DEFAULT_GHA_CHUNK_SIZE},
    GhaBackend,
};
use anyhow::Result;
use sha256::digest;
use std::path::Path;
use tokio::fs::{read, write};
use tonic::{async_trait, Status};
use tracing::info;
use vorpal_schema::artifact::v0::{Artifact, ArtifactRequest};

#[async_trait]
impl ArtifactBackend for GhaBackend {
    async fn get_artifact(&self, request: &ArtifactRequest) -> Result<Artifact, Status> {
        let request_key = get_artifact_key(&request.digest);
        let request_file = format!("/tmp/{}", request_key);
        let request_path = Path::new(&request_file);

        if request_path.exists() {
            let data = read(&request_path)
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            let artifact = serde_json::from_slice::<Artifact>(&data)
                .map_err(|err| Status::internal(err.to_string()))?;

            return Ok(artifact);
        }

        info!("fetch cache: {}", request_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&request_key, &request.digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            return Err(Status::not_found(format!(
                "cache entry not found: {request_key}"
            )));
        }

        let cache_entry = cache_entry.as_ref().unwrap();

        info!("fetch cache location: {:?}", cache_entry.archive_location);

        let cache_response = reqwest::get(&cache_entry.archive_location)
            .await
            .expect("failed to get");

        let cache_response_bytes = cache_response
            .bytes()
            .await
            .expect("failed to read response");

        write(&request_path, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("fetch cache saved: {:?}", request_path);

        let data = read(&request_path)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let artifact = serde_json::from_slice::<Artifact>(&data)
            .map_err(|err| Status::internal(err.to_string()))?;

        Ok(artifact)
    }

    async fn store_artifact(&self, request: &Artifact) -> Result<String, Status> {
        let request_json = serde_json::to_vec(request)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let request_digest = digest(&request_json);
        let request_key = get_artifact_key(&request_digest);
        let request_file = format!("/tmp/{}", request_key);
        let request_path = Path::new(&request_file);

        if !request_path.exists() {
            write(request_path, &request_json)
                .await
                .map_err(|err| Status::internal(format!("failed to write artifact: {:?}", err)))?;
        }

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&request_key, &request_digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            let cache_size = request_json.len() as u64;

            let cache_reserve = &self
                .cache_client
                .reserve_cache(request_key, request_digest.clone(), Some(cache_size))
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to reserve cache: {:?}", e.to_string()))
                })?;

            if cache_reserve.cache_id == 0 {
                return Err(Status::internal("failed to reserve cache returned 0"));
            }

            self.cache_client
                .save_cache(
                    cache_reserve.cache_id,
                    &request_json,
                    DEFAULT_GHA_CHUNK_SIZE,
                )
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to save cache: {:?}", e.to_string()))
                })?;
        }

        Ok(request_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
