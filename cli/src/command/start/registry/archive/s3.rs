use crate::command::start::registry::{s3::get_archive_key, ArchiveBackend, S3Backend};
use tokio::sync::mpsc;
use tonic::{async_trait, Status};
use vorpal_sdk::api::archive::{ArchivePullRequest, ArchivePullResponse, ArchivePushRequest};

#[async_trait]
impl ArchiveBackend for S3Backend {
    async fn check(&self, request: &ArchivePullRequest) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let request_key = get_archive_key(&request.digest);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(request_key.clone())
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        Ok(())
    }

    async fn pull(
        &self,
        request: &ArchivePullRequest,
        tx: mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let request_key = get_archive_key(&request.digest);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(request_key.clone())
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        let mut archive_stream = client
            .get_object()
            .bucket(bucket)
            .key(request_key)
            .send()
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .body;

        while let Some(chunk) = archive_stream.next().await {
            let archive_chunk = chunk.map_err(|err| Status::internal(err.to_string()))?;

            tx.send(Ok(ArchivePullResponse {
                data: archive_chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        Ok(())
    }

    async fn push(&self, request: &ArchivePushRequest) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let request_key = get_archive_key(&request.digest);
        let request_head = client
            .head_object()
            .bucket(bucket)
            .key(&request_key)
            .send()
            .await;

        if request_head.is_ok() {
            return Ok(());
        }

        client
            .put_object()
            .bucket(bucket)
            .key(request_key)
            .body(request.data.clone().into())
            .send()
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn ArchiveBackend> {
        Box::new(self.clone())
    }
}
