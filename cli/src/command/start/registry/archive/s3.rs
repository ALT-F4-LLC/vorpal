use crate::command::start::registry::{s3::get_artifact_archive_key, ArchiveBackend, S3Backend};
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use bytes::BytesMut;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tonic::{async_trait, Status};
use tracing::{info, warn};
use vorpal_sdk::api::archive::{ArchivePullRequest, ArchivePullResponse};

/// S3 minimum part size for multipart uploads (5 MiB).
const S3_MIN_PART_SIZE: usize = 5 * 1024 * 1024;

#[async_trait]
impl ArchiveBackend for S3Backend {
    async fn check(&self, request: &ArchivePullRequest) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let archive_key = get_artifact_archive_key(&request.digest, &request.namespace);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(archive_key.clone())
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

        let archive_key = get_artifact_archive_key(&request.digest, &request.namespace);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(archive_key.clone())
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        let mut archive_stream = client
            .get_object()
            .bucket(bucket)
            .key(archive_key)
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
        let client = &self.client;
        let bucket = &self.bucket;
        let archive_key = get_artifact_archive_key(digest, namespace);

        // Idempotent: short-circuit if archive already exists.
        if client
            .head_object()
            .bucket(bucket.clone())
            .key(archive_key.clone())
            .send()
            .await
            .is_ok()
        {
            return Ok(());
        }

        // Buffer incoming chunks. We decide between single PutObject and multipart
        // after the stream ends or the buffer exceeds S3_MIN_PART_SIZE.
        let mut buffer = BytesMut::new();
        let mut completed_parts: Vec<CompletedPart> = Vec::new();
        let mut upload_id: Option<String> = None;
        let mut part_number: i32 = 1;

        let result: Result<(), Status> = async {
            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result?;
                buffer.extend_from_slice(&chunk);

                if buffer.len() >= S3_MIN_PART_SIZE {
                    // Lazily initiate multipart upload on first full buffer.
                    if upload_id.is_none() {
                        let create_resp = client
                            .create_multipart_upload()
                            .bucket(bucket.clone())
                            .key(archive_key.clone())
                            .send()
                            .await
                            .map_err(|err| {
                                Status::internal(format!(
                                    "failed to create multipart upload: {err}"
                                ))
                            })?;

                        upload_id = create_resp.upload_id().map(|s| s.to_string());
                    }

                    let uid = upload_id
                        .as_ref()
                        .ok_or_else(|| Status::internal("S3 did not return an upload ID"))?;

                    // Drain the buffer up to full parts.
                    while buffer.len() >= S3_MIN_PART_SIZE {
                        let part_data = buffer.split_to(S3_MIN_PART_SIZE).freeze();

                        let upload_resp = client
                            .upload_part()
                            .bucket(bucket.clone())
                            .key(archive_key.clone())
                            .upload_id(uid.clone())
                            .part_number(part_number)
                            .body(part_data.into())
                            .send()
                            .await
                            .map_err(|err| {
                                Status::internal(format!(
                                    "failed to upload part {part_number}: {err}"
                                ))
                            })?;

                        completed_parts.push(
                            CompletedPart::builder()
                                .e_tag(upload_resp.e_tag().unwrap_or_default())
                                .part_number(part_number)
                                .build(),
                        );

                        part_number += 1;
                    }
                }
            }

            // Stream ended. Handle remaining data.
            if upload_id.is_none() {
                // Total data < 5MB — use single PutObject.
                let body = buffer.freeze();
                client
                    .put_object()
                    .bucket(bucket.clone())
                    .key(archive_key.clone())
                    .body(body.into())
                    .send()
                    .await
                    .map_err(|err| Status::internal(format!("failed to put object: {err}")))?;

                info!("registry |> archive push (single put): {archive_key}");
                return Ok(());
            }

            // Upload remaining buffer as the final part.
            let uid = upload_id.as_ref().unwrap();

            if !buffer.is_empty() {
                let part_data = buffer.freeze();

                let upload_resp = client
                    .upload_part()
                    .bucket(bucket.clone())
                    .key(archive_key.clone())
                    .upload_id(uid.clone())
                    .part_number(part_number)
                    .body(part_data.into())
                    .send()
                    .await
                    .map_err(|err| {
                        Status::internal(format!(
                            "failed to upload final part {part_number}: {err}"
                        ))
                    })?;

                completed_parts.push(
                    CompletedPart::builder()
                        .e_tag(upload_resp.e_tag().unwrap_or_default())
                        .part_number(part_number)
                        .build(),
                );
            }

            // Complete multipart upload.
            let completed_upload = CompletedMultipartUpload::builder()
                .set_parts(Some(completed_parts.clone()))
                .build();

            client
                .complete_multipart_upload()
                .bucket(bucket.clone())
                .key(archive_key.clone())
                .upload_id(uid.clone())
                .multipart_upload(completed_upload)
                .send()
                .await
                .map_err(|err| {
                    Status::internal(format!("failed to complete multipart upload: {err}"))
                })?;

            info!(
                "registry |> archive push (multipart, {} parts): {archive_key}",
                completed_parts.len()
            );

            Ok(())
        }
        .await;

        // On ANY error after multipart was initiated, abort the upload.
        if result.is_err() {
            if let Some(uid) = &upload_id {
                warn!("registry |> aborting multipart upload {uid} for {archive_key}");

                if let Err(abort_err) = client
                    .abort_multipart_upload()
                    .bucket(bucket.clone())
                    .key(archive_key.clone())
                    .upload_id(uid.clone())
                    .send()
                    .await
                {
                    warn!("registry |> failed to abort multipart upload {uid}: {abort_err}");
                }
            }
        }

        result
    }

    fn box_clone(&self) -> Box<dyn ArchiveBackend> {
        Box::new(self.clone())
    }
}
