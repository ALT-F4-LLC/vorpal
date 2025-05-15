use crate::command::{
    start::registry::{
        gha::{get_artifact_alias_key, get_artifact_config_key, DEFAULT_GHA_CHUNK_SIZE},
        ArtifactBackend, GhaBackend,
    },
    store::paths::{get_artifact_alias_path, get_artifact_config_path, set_timestamps},
};
use anyhow::Result;
use sha256::digest;
use tokio::fs::{create_dir_all, read, write};
use tonic::{async_trait, Status};
use tracing::info;
use vorpal_sdk::api::artifact::Artifact;

#[async_trait]
impl ArtifactBackend for GhaBackend {
    async fn get_artifact(&self, artifact_digest: String) -> Result<Artifact, Status> {
        let config_path = get_artifact_config_path(&artifact_digest);

        if config_path.exists() {
            let config_data = read(&config_path)
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            let artifact = serde_json::from_slice::<Artifact>(&config_data)
                .map_err(|err| Status::internal(err.to_string()))?;

            return Ok(artifact);
        }

        let config_key = get_artifact_config_key(&artifact_digest);

        info!("cache: {}", config_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&config_key, &artifact_digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            return Err(Status::not_found(format!(
                "cache entry not found: {config_key}"
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

        write(&config_path, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("cache written: {:?}", config_path);

        let config_data = read(&config_path)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        let artifact = serde_json::from_slice::<Artifact>(&config_data)
            .map_err(|err| Status::internal(err.to_string()))?;

        Ok(artifact)
    }

    async fn store_artifact(
        &self,
        artifact: Artifact,
        artifact_alias: Option<String>,
    ) -> Result<String, Status> {
        let config_json = serde_json::to_vec(&artifact)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let config_digest = digest(&config_json);
        let config_path = get_artifact_config_path(&config_digest);

        if !config_path.exists() {
            write(config_path, &config_json)
                .await
                .map_err(|err| Status::internal(format!("failed to write artifact: {:?}", err)))?;
        }

        let config_key = get_artifact_config_key(&config_digest);

        info!("cache: {}", config_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&config_key, &config_digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            let cache_size = config_json.len() as u64;

            let cache_reserve = &self
                .cache_client
                .reserve_cache(config_key.clone(), config_digest.clone(), Some(cache_size))
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to reserve cache: {:?}", e.to_string()))
                })?;

            if cache_reserve.cache_id == 0 {
                return Err(Status::internal("failed to reserve cache returned 0"));
            }

            info!("cache reserved: {:?}", cache_reserve.cache_id);

            self.cache_client
                .save_cache(cache_reserve.cache_id, &config_json, DEFAULT_GHA_CHUNK_SIZE)
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to save cache: {:?}", e.to_string()))
                })?;

            info!("cache saved: {:?}", cache_reserve.cache_id);
        }

        if let Some(alias) = artifact_alias {
            let alias_path = get_artifact_alias_path(&alias).map_err(|err| {
                Status::internal(format!("failed to get artifact alias path: {:?}", err))
            })?;

            if let Some(parent) = alias_path.parent() {
                if !parent.exists() {
                    create_dir_all(parent).await.map_err(|err| {
                        Status::internal(format!("failed to create alias dir: {:?}", err))
                    })?;
                }
            }

            write(&alias_path, &config_digest)
                .await
                .map_err(|err| Status::internal(format!("failed to write alias: {:?}", err)))?;

            set_timestamps(&alias_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize alias: {:?}", err)))?;

            let alias_key = get_artifact_alias_key(&alias).map_err(|err| {
                Status::internal(format!("failed to get artifact alias key: {:?}", err))
            })?;

            let cache_entry = &self
                .cache_client
                .get_cache_entry(&alias_key, &config_digest)
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
                })?;

            if cache_entry.is_none() {
                let cache_size = config_digest.len() as u64;

                let cache_reserve = &self
                    .cache_client
                    .reserve_cache(alias_key.clone(), config_digest.clone(), Some(cache_size))
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
                        config_digest.as_bytes(),
                        DEFAULT_GHA_CHUNK_SIZE,
                    )
                    .await
                    .map_err(|e| {
                        Status::internal(format!("failed to save cache: {:?}", e.to_string()))
                    })?;

                info!("cache saved: {:?}", cache_reserve.cache_id);
            }
        }

        Ok(config_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
