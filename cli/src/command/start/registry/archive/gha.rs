use crate::command::{
    start::registry::{
        gha::{get_artifact_archive_key, DEFAULT_GHA_CHUNK_SIZE},
        ArchiveBackend, GhaBackend, DEFAULT_GRPC_CHUNK_SIZE,
    },
    store::paths::get_artifact_archive_path,
};
use anyhow::Result;
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use tracing::info;
use vorpal_sdk::api::archive::{ArchivePullRequest, ArchivePullResponse, ArchivePushRequest};

#[async_trait]
impl ArchiveBackend for GhaBackend {
    async fn check(&self, request: &ArchivePullRequest) -> Result<(), Status> {
        let archive_path = get_artifact_archive_path(&request.digest);

        if archive_path.exists() {
            return Ok(());
        }

        let archive_key = get_artifact_archive_key(&request.digest);

        info!("cache: {}", archive_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&archive_key, &request.digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            return Err(Status::not_found(format!(
                "cache entry not found: {archive_key}"
            )));
        }

        Ok(())
    }

    async fn pull(
        &self,
        request: &ArchivePullRequest,
        tx: mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status> {
        let archive_path = get_artifact_archive_path(&request.digest);

        if archive_path.exists() {
            let archive_data = read(&archive_path)
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            for chunk in archive_data.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
                tx.send(Ok(ArchivePullResponse {
                    data: chunk.to_vec(),
                }))
                .await
                .map_err(|err| {
                    Status::internal(format!("failed to send store chunk: {:?}", err))
                })?;
            }

            return Ok(());
        }

        let archive_key = get_artifact_archive_key(&request.digest);

        info!("cache: {}", archive_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&archive_key, &request.digest)
            .await
            .expect("failed to get cache entry");

        let Some(cache_entry) = cache_entry else {
            return Err(Status::not_found("store path not found"));
        };

        info!("cache location: {:?}", cache_entry.archive_location);

        let cache_response = reqwest::get(&cache_entry.archive_location)
            .await
            .expect("failed to get");

        let cache_response_bytes = cache_response
            .bytes()
            .await
            .expect("failed to read response");

        write(&archive_path, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("cache written: {:?}", archive_path);

        for chunk in cache_response_bytes.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
            tx.send(Ok(ArchivePullResponse {
                data: chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        info!("cache sent: {:?}", archive_path);

        Ok(())
    }

    async fn push(&self, request: &ArchivePushRequest) -> Result<(), Status> {
        let archive_path = get_artifact_archive_path(&request.digest);

        if !archive_path.exists() {
            write(archive_path, &request.data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;
        }

        let archive_key = get_artifact_archive_key(request.digest.as_str());

        info!("cache: {}", archive_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&archive_key, &request.digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            let cache_size = request.data.len() as u64;

            let cache_reserve = &self
                .cache_client
                .reserve_cache(archive_key, request.digest.clone(), Some(cache_size))
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
                    &request.data,
                    DEFAULT_GHA_CHUNK_SIZE,
                )
                .await
                .map_err(|e| {
                    Status::internal(format!("failed to save cache: {:?}", e.to_string()))
                })?;

            info!("cache saved: {:?}", cache_reserve.cache_id);
        }

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn ArchiveBackend> {
        Box::new(self.clone())
    }
}
