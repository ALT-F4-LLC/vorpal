use crate::command::{
    start::registry::{ArchiveBackend, LocalBackend, DEFAULT_GRPC_CHUNK_SIZE},
    store::paths::{get_artifact_archive_path, set_timestamps},
};
use tokio::{
    fs::{create_dir_all, read, remove_file, rename},
    io::{AsyncWriteExt, BufWriter},
    sync::mpsc,
};
use tokio_stream::{Stream, StreamExt};
use tonic::{async_trait, Status};
use uuid::Uuid;
use vorpal_sdk::api::archive::{ArchivePullRequest, ArchivePullResponse};

#[async_trait]
impl ArchiveBackend for LocalBackend {
    async fn check(&self, request: &ArchivePullRequest) -> Result<(), Status> {
        let request_path = get_artifact_archive_path(&request.digest, &request.namespace);

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
        let request_path = get_artifact_archive_path(&request.digest, &request.namespace);

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
            .map_err(|err| Status::internal(format!("failed to send store chunk: {err}")))?;
        }

        Ok(())
    }

    async fn push(
        &self,
        digest: &str,
        namespace: &str,
        stream: &mut (dyn Stream<Item = Result<bytes::Bytes, Status>> + Unpin + Send),
    ) -> Result<(), Status> {
        let final_path = get_artifact_archive_path(digest, namespace);

        // Idempotent: if the archive already exists, nothing to do.
        if final_path.exists() {
            return Ok(());
        }

        // Ensure parent directory exists.
        let parent = final_path
            .parent()
            .ok_or_else(|| Status::internal("archive path has no parent directory"))?;

        create_dir_all(parent)
            .await
            .map_err(|e| Status::internal(format!("failed to create archive directory: {e}")))?;

        // Create temp file in the same directory for atomic rename.
        let temp_path = parent.join(format!(
            "{}.{}.tmp",
            final_path.file_name().unwrap_or_default().to_string_lossy(),
            Uuid::now_v7()
        ));

        // Write stream chunks to temp file, cleaning up on any error.
        let result = async {
            let file = tokio::fs::File::create(&temp_path)
                .await
                .map_err(|e| Status::internal(format!("failed to create temp file: {e}")))?;

            let mut writer = BufWriter::new(file);

            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                writer
                    .write_all(&chunk)
                    .await
                    .map_err(|e| Status::internal(format!("failed to write chunk: {e}")))?;
            }

            writer
                .flush()
                .await
                .map_err(|e| Status::internal(format!("failed to flush writer: {e}")))?;

            writer
                .into_inner()
                .sync_all()
                .await
                .map_err(|e| Status::internal(format!("failed to sync file: {e}")))?;

            rename(&temp_path, &final_path)
                .await
                .map_err(|e| Status::internal(format!("failed to rename temp file: {e}")))?;

            set_timestamps(&final_path)
                .await
                .map_err(|e| Status::internal(format!("failed to set timestamps: {e}")))?;

            Ok(())
        }
        .await;

        // On any error, clean up the temp file.
        if result.is_err() {
            let _ = remove_file(&temp_path).await;
        }

        result
    }

    fn box_clone(&self) -> Box<dyn ArchiveBackend> {
        Box::new(self.clone())
    }
}
