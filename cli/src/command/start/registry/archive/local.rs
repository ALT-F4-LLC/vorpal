use crate::command::{
    start::registry::{ArchiveBackend, LocalBackend, DEFAULT_GRPC_CHUNK_SIZE},
    store::paths::{get_artifact_archive_path, set_timestamps},
};
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use vorpal_sdk::api::archive::{ArchivePullRequest, ArchivePullResponse, ArchivePushRequest};

#[async_trait]
impl ArchiveBackend for LocalBackend {
    async fn check(&self, request: &ArchivePullRequest) -> Result<(), Status> {
        let request_path = get_artifact_archive_path(&request.digest);

        if !request_path.exists() {
            return Err(Status::not_found("archive not found"));
        }

        Ok(())
    }

    async fn pull(
        &self,
        request: &ArchivePullRequest,
        tx: mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status> {
        let request_path = get_artifact_archive_path(&request.digest);

        if !request_path.exists() {
            return Err(Status::not_found("archive not found"));
        }

        let archive_data = read(&request_path)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        for chunk in archive_data.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
            tx.send(Ok(ArchivePullResponse {
                data: chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        Ok(())
    }

    async fn push(&self, request: &ArchivePushRequest) -> Result<(), Status> {
        let request_path = get_artifact_archive_path(&request.digest);

        if !request_path.exists() {
            write(&request_path, &request.data).await.map_err(|err| {
                Status::internal(format!("failed to write store path: {:?}", err))
            })?;

            set_timestamps(&request_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;
        }

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn ArchiveBackend> {
        Box::new(self.clone())
    }
}
