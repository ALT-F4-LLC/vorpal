use crate::command::start::registry::{s3::get_artifact_archive_key, ArchiveBackend, S3Backend};
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client;
use bytes::BytesMut;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tonic::{async_trait, Status};
use tracing::{info, warn};
use vorpal_sdk::api::archive::{ArchivePullRequest, ArchivePullResponse};

/// S3 minimum part size for multipart uploads (5 MiB).
const S3_MIN_PART_SIZE: usize = 5 * 1024 * 1024;

/// Drains `buffer` down to full `S3_MIN_PART_SIZE` parts, uploading each one, lazily
/// initiating the multipart upload on the first full buffer if `upload_id` is not yet
/// set. On return, `buffer` holds fewer than `S3_MIN_PART_SIZE` bytes.
async fn upload_full_parts(
    client: &Client,
    bucket: &str,
    archive_key: &str,
    buffer: &mut BytesMut,
    upload_id: &mut Option<String>,
    completed_parts: &mut Vec<CompletedPart>,
    part_number: &mut i32,
) -> Result<(), Status> {
    if upload_id.is_none() {
        let create_resp = client
            .create_multipart_upload()
            .bucket(bucket)
            .key(archive_key)
            .send()
            .await
            .map_err(|err| Status::internal(format!("failed to create multipart upload: {err}")))?;

        *upload_id = create_resp
            .upload_id()
            .map(std::string::ToString::to_string);
    }

    let Some(uid) = upload_id.as_ref() else {
        return Err(Status::internal("S3 did not return an upload ID"));
    };

    while buffer.len() >= S3_MIN_PART_SIZE {
        let part_data = buffer.split_to(S3_MIN_PART_SIZE).freeze();

        let upload_resp = client
            .upload_part()
            .bucket(bucket)
            .key(archive_key)
            .upload_id(uid)
            .part_number(*part_number)
            .body(part_data.into())
            .send()
            .await
            .map_err(|err| {
                Status::internal(format!("failed to upload part {part_number}: {err}"))
            })?;

        completed_parts.push(
            CompletedPart::builder()
                .e_tag(upload_resp.e_tag().unwrap_or_default())
                .part_number(*part_number)
                .build(),
        );

        *part_number += 1;
    }

    Ok(())
}

/// Finishes an archive upload once the source stream has ended: if no multipart upload
/// was started, puts `buffer` as a single object; otherwise uploads any remaining buffered
/// bytes as the final part and completes the multipart upload.
async fn finish_upload(
    client: &Client,
    bucket: &str,
    archive_key: &str,
    buffer: BytesMut,
    upload_id: Option<&str>,
    mut completed_parts: Vec<CompletedPart>,
    part_number: i32,
) -> Result<(), Status> {
    let Some(uid) = upload_id else {
        // Total data < 5MB — use single PutObject.
        let body = buffer.freeze();
        client
            .put_object()
            .bucket(bucket)
            .key(archive_key)
            .body(body.into())
            .send()
            .await
            .map_err(|err| Status::internal(format!("failed to put object: {err}")))?;

        info!("registry |> archive push (single put): {archive_key}");
        return Ok(());
    };

    // Upload remaining buffer as the final part.

    if !buffer.is_empty() {
        let part_data = buffer.freeze();

        let upload_resp = client
            .upload_part()
            .bucket(bucket)
            .key(archive_key)
            .upload_id(uid)
            .part_number(part_number)
            .body(part_data.into())
            .send()
            .await
            .map_err(|err| {
                Status::internal(format!("failed to upload final part {part_number}: {err}"))
            })?;

        completed_parts.push(
            CompletedPart::builder()
                .e_tag(upload_resp.e_tag().unwrap_or_default())
                .part_number(part_number)
                .build(),
        );
    }

    // Complete multipart upload.
    let completed_parts_len = completed_parts.len();
    let completed_upload = CompletedMultipartUpload::builder()
        .set_parts(Some(completed_parts))
        .build();

    client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(archive_key)
        .upload_id(uid)
        .multipart_upload(completed_upload)
        .send()
        .await
        .map_err(|err| Status::internal(format!("failed to complete multipart upload: {err}")))?;

    info!("registry |> archive push (multipart, {completed_parts_len} parts): {archive_key}");

    Ok(())
}

#[async_trait]
impl ArchiveBackend for S3Backend {
    async fn check(&self, request: &ArchivePullRequest) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let archive_key = get_artifact_archive_key(&request.digest, &request.namespace);

        client
            .head_object()
            .bucket(bucket)
            .key(archive_key)
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        Ok(())
    }

    async fn pull(
        &self,
        request: &ArchivePullRequest,
        tx: &mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let archive_key = get_artifact_archive_key(&request.digest, &request.namespace);

        // archive_key is reused below (get_object); bucket is a borrow of self,
        // reusable as-is since the S3 builder accepts &String via Into<String>.
        client
            .head_object()
            .bucket(bucket)
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

        // Idempotent: short-circuit if archive already exists. bucket/archive_key
        // are reused below (upload_full_parts/finish_upload), so only borrow here.
        if client
            .head_object()
            .bucket(bucket)
            .key(&archive_key)
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
                    upload_full_parts(
                        client,
                        bucket,
                        &archive_key,
                        &mut buffer,
                        &mut upload_id,
                        &mut completed_parts,
                        &mut part_number,
                    )
                    .await?;
                }
            }

            // Stream ended. Finish the upload (single put or multipart completion).
            finish_upload(
                client,
                bucket,
                &archive_key,
                buffer,
                upload_id.as_deref(),
                completed_parts,
                part_number,
            )
            .await
        }
        .await;

        // On ANY error after multipart was initiated, abort the upload.
        if result.is_err() {
            if let Some(uid) = &upload_id {
                warn!("registry |> aborting multipart upload {uid} for {archive_key}");

                if let Err(abort_err) = client
                    .abort_multipart_upload()
                    .bucket(bucket)
                    .key(archive_key)
                    .upload_id(uid)
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
