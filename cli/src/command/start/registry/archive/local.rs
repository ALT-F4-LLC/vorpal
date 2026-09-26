use crate::command::{
    start::registry::{ArchiveBackend, LocalBackend, DEFAULT_GRPC_CHUNK_SIZE},
    store::paths::set_timestamps,
};
use tokio::{
    fs::{create_dir_all, read, remove_file, rename},
    io::{AsyncWriteExt, BufWriter},
    sync::mpsc,
};
use tokio_stream::{Stream, StreamExt};
use tonic::{async_trait, Status};
use uuid::Uuid;
use vorpal_sdk::api::archive::ArchivePullResponse;

#[async_trait]
impl ArchiveBackend for LocalBackend {
    async fn check(&self, digest: &str, namespace: &str) -> Result<(), Status> {
        let request_path = self.archive_path(digest, namespace);

        if !request_path.exists() {
            return Err(Status::not_found("archive not found"));
        }

        Ok(())
    }

    async fn pull(
        &self,
        digest: &str,
        namespace: &str,
        tx: &mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status> {
        let request_path = self.archive_path(digest, namespace);

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
        let final_path = self.archive_path(digest, namespace);

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

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "test assertions read as intent, not defensive code: an unwrap failure is the test failing, which is the point"
)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;
    use tempfile::TempDir;

    const DIGEST: &str = "d69f5fdcd82b6a3d5aae91ec92be4c30bd1c5f8ff0e5e6cea9cbe2c3cbcdd3fd";
    const NAMESPACE: &str = "library";

    fn dir_entry_names(dir: &Path) -> BTreeSet<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn byte_stream(
        chunks: Vec<Result<bytes::Bytes, Status>>,
    ) -> impl Stream<Item = Result<bytes::Bytes, Status>> + Unpin + Send {
        tokio_stream::iter(chunks)
    }

    // A client that dies mid-body leaves a partially written temp file behind.
    // The push must fail and the namespace directory must hold nothing at all:
    // no temp file for a later listing to trip over, and no archive at the
    // final path that a subsequent pull would serve as a complete artifact.
    //
    // The stream captures the directory listing it sees just before yielding
    // the error, so the empty listing afterwards establishes that cleanup
    // removed a temp file that was really there: an empty directory alone
    // would also be produced by a push that never opened one.
    #[tokio::test]
    async fn a_stream_error_mid_body_removes_the_temp_file_it_had_written() {
        let root = TempDir::new().unwrap();
        let backend = LocalBackend::new(root.path().to_path_buf());
        let namespace_dir = root.path().join("archive").join(NAMESPACE);

        let observed_dir = namespace_dir.clone();
        let observed_mid_stream = std::sync::Arc::new(std::sync::Mutex::new(BTreeSet::new()));
        let observer = std::sync::Arc::clone(&observed_mid_stream);

        let mut stream = Box::pin(
            tokio_stream::iter(vec![Ok(bytes::Bytes::from_static(b"first-half"))]).chain(
                tokio_stream::once(()).map(move |()| {
                    *observer.lock().unwrap() = dir_entry_names(&observed_dir);
                    Err(Status::internal("client disconnected"))
                }),
            ),
        );

        let status = backend
            .push(DIGEST, NAMESPACE, &mut stream)
            .await
            .unwrap_err();

        assert_eq!(status.code(), tonic::Code::Internal);

        let mid_stream = observed_mid_stream.lock().unwrap().clone();

        assert_eq!(
            mid_stream.len(),
            1,
            "the push must have opened a temp file before the stream failed"
        );
        assert!(std::path::Path::new(mid_stream.iter().next().unwrap())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp")));

        assert_eq!(
            dir_entry_names(&namespace_dir),
            BTreeSet::new(),
            "the failed push must leave neither a temp file nor an archive"
        );
        assert!(backend.check(DIGEST, NAMESPACE).await.is_err());
    }

    // The positive control for the cleanup above: a stream that completes must
    // leave exactly the final archive holding every chunk's bytes, so an empty
    // directory cannot be mistaken for a push that never wrote anything.
    #[tokio::test]
    async fn a_complete_stream_leaves_exactly_the_final_archive() {
        let root = TempDir::new().unwrap();
        let backend = LocalBackend::new(root.path().to_path_buf());

        let mut stream = byte_stream(vec![
            Ok(bytes::Bytes::from_static(b"first-half")),
            Ok(bytes::Bytes::from_static(b"second-half")),
        ]);

        backend.push(DIGEST, NAMESPACE, &mut stream).await.unwrap();

        let archive_path = root
            .path()
            .join("archive")
            .join(NAMESPACE)
            .join(format!("{DIGEST}.tar.zst"));

        assert_eq!(
            dir_entry_names(&root.path().join("archive").join(NAMESPACE)),
            BTreeSet::from([format!("{DIGEST}.tar.zst")]),
            "the archive must be the only entry the push leaves"
        );
        assert_eq!(
            std::fs::read(&archive_path).unwrap(),
            b"first-halfsecond-half"
        );

        backend.check(DIGEST, NAMESPACE).await.unwrap();
    }
}
