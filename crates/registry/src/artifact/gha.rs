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
use vorpal_schema::artifact::v0::Artifact;

#[async_trait]
impl ArtifactBackend for GhaBackend {
    async fn get_artifact(&self, artifact_digest: String) -> Result<Artifact, Status> {
        let artifact_key = get_artifact_key(&artifact_digest);
        let artifact_file = format!("/tmp/{}", artifact_key);
        let artifact_path = Path::new(&artifact_file);

        if artifact_path.exists() {
            let artifact_data = read(&artifact_path)
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            let artifact = serde_json::from_slice::<Artifact>(&artifact_data)
                .map_err(|err| Status::internal(err.to_string()))?;

            return Ok(artifact);
        }

        info!("cache: {}", artifact_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&artifact_key, &artifact_digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            return Err(Status::not_found(format!(
                "cache entry not found: {artifact_key}"
            )));
        }

        let cache_entry = cache_entry.as_ref().unwrap();

        info!("cache location: {:?}", cache_entry.archive_location);

        let cache_response = reqwest::get(&cache_entry.archive_location)
            .await
            .expect("failed to get");

        let cache_response_bytes = cache_response
            .bytes()
            .await
            .expect("failed to read response");

        write(&artifact_path, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("cache written: {:?}", artifact_path);

        let artifact_data = read(&artifact_path)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let artifact = serde_json::from_slice::<Artifact>(&artifact_data)
            .map_err(|err| Status::internal(err.to_string()))?;

        Ok(artifact)
    }

    async fn store_artifact(&self, artifact: &Artifact) -> Result<String, Status> {
        let artifact_json = serde_json::to_vec(artifact)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let artifact_digest = digest(&artifact_json);
        let artifact_key = get_artifact_key(&artifact_digest);
        let artifact_file = format!("/tmp/{}", artifact_key);
        let artifact_path = Path::new(&artifact_file);

        if !artifact_path.exists() {
            write(artifact_path, &artifact_json)
                .await
                .map_err(|err| Status::internal(format!("failed to write artifact: {:?}", err)))?;
        }

        info!("cache: {}", artifact_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&artifact_key, &artifact_digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            let cache_size = artifact_json.len() as u64;

            let cache_reserve = &self
                .cache_client
                .reserve_cache(
                    artifact_key.clone(),
                    artifact_digest.clone(),
                    Some(cache_size),
                )
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to reserve cache: {:?}", e.to_string()))
                })?;

            if cache_reserve.cache_id == 0 {
                return Err(Status::internal("failed to reserve cache returned 0"));
            }

            info!("cache reserved: {:?}", cache_reserve.cache_id);

            self.cache_client
                .save_cache(
                    cache_reserve.cache_id,
                    &artifact_json,
                    DEFAULT_GHA_CHUNK_SIZE,
                )
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to save cache: {:?}", e.to_string()))
                })?;

            info!("cache saved: {:?}", cache_reserve.cache_id);
        }

        Ok(artifact_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
