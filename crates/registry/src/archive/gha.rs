use crate::{
    archive::{ArchiveBackend, DEFAULT_GRPC_CHUNK_SIZE},
    gha::{get_archive_key, DEFAULT_GHA_CHUNK_SIZE},
    GhaBackend,
};
use anyhow::Result;
use std::path::Path;
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use tracing::info;
use vorpal_schema::archive::v0::{ArchivePullRequest, ArchivePullResponse, ArchivePushRequest};

#[async_trait]
impl ArchiveBackend for GhaBackend {
    async fn check(&self, request: &ArchivePullRequest) -> Result<(), Status> {
        let request_key = get_archive_key(&request.digest);
        let request_file = format!("/tmp/{}", request_key);
        let request_path = Path::new(&request_file);

        if request_path.exists() {
            return Ok(());
        }

        info!("cache: {}", request_key);

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

        Ok(())
    }

    async fn pull(
        &self,
        request: &ArchivePullRequest,
        tx: mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status> {
        let request_key = get_archive_key(&request.digest);
        let request_file = format!("/tmp/{}", request_key);
        let request_path = Path::new(&request_file);

        if request_path.exists() {
            let archive_data = read(&request_path)
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

        info!("cache: {}", request_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&request_key, &request.digest)
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

        write(&request_path, &cache_response_bytes)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        info!("cache written: {:?}", request_path);

        for chunk in cache_response_bytes.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
            tx.send(Ok(ArchivePullResponse {
                data: chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        info!("cache sent: {:?}", request_path);

        Ok(())
    }

    async fn push(&self, request: &ArchivePushRequest) -> Result<(), Status> {
        let request_key = get_archive_key(request.digest.as_str());
        let request_file = format!("/tmp/{}", request_key);
        let request_path = Path::new(&request_file);

        if !request_path.exists() {
            write(request_path, &request.data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;
        }

        info!("cache: {}", request_key);

        let cache_entry = &self
            .cache_client
            .get_cache_entry(&request_key, &request.digest)
            .await
            .map_err(|e| {
                Status::internal(format!("failed to get cache entry: {:?}", e.to_string()))
            })?;

        if cache_entry.is_none() {
            let cache_size = request.data.len() as u64;

            let cache_reserve = &self
                .cache_client
                .reserve_cache(request_key, request.digest.clone(), Some(cache_size))
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
