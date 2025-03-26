use aws_sdk_s3::Client;
use tokio::sync::mpsc;
use tonic::{async_trait, Status};
use vorpal_schema::vorpal::registry::v0::{RegistryKind, RegistryPullResponse, RegistryRequest};
use vorpal_store::paths::get_store_dir_name;

use crate::{PushMetadata, RegistryBackend, RegistryError};

#[derive(Clone, Debug)]
pub struct S3RegistryBackend {
    bucket: String,
    client: Client,
}

impl S3RegistryBackend {
    pub async fn new(bucket: Option<String>) -> Result<Self, RegistryError> {
        let Some(bucket) = bucket else {
            return Err(RegistryError::MissingS3Bucket);
        };

        let client_config = aws_config::load_from_env().await;
        let client = Client::new(&client_config);

        Ok(Self { bucket, client })
    }
}

fn get_artifact_key(kind: RegistryKind, hash: &str, name: &str) -> Result<String, Status> {
    match kind {
        RegistryKind::Artifact => Ok(format!("store/{}.artifact", get_store_dir_name(hash, name))),
        RegistryKind::ArtifactSource => {
            Ok(format!("store/{}.source", get_store_dir_name(hash, name)))
        }
        _ => Err(Status::invalid_argument("unsupported store kind")),
    }
}

fn get_manifest_key(kind: RegistryKind, hash: &str, name: &str) -> Result<String, Status> {
    match kind {
        RegistryKind::Artifact => Ok(format!("store/{}.artifact", get_store_dir_name(hash, name))),
        RegistryKind::ArtifactSource => {
            Ok(format!("store/{}.source", get_store_dir_name(hash, name)))
        }
        _ => Err(Status::invalid_argument("unsupported store kind")),
    }
}

#[async_trait]
impl RegistryBackend for S3RegistryBackend {
    async fn exists(&self, request: &RegistryRequest) -> Result<String, Status> {
        let manifest_key = get_manifest_key(request.kind(), &request.hash, &request.name)?;

        let client = &self.client;
        let bucket = &self.bucket;

        client
            .head_object()
            .bucket(bucket.clone())
            .key(&manifest_key)
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        let mut stream = client
            .get_object()
            .bucket(bucket)
            .key(&manifest_key)
            .send()
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .body;

        let mut manifest = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|err| Status::internal(err.to_string()))?;

            manifest.push_str(&String::from_utf8_lossy(&chunk));
        }

        Ok(manifest)
    }

    async fn pull(
        &self,
        request: &RegistryRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_key = get_artifact_key(request.kind(), &request.hash, &request.name)?;

        client
            .head_object()
            .bucket(bucket.clone())
            .key(artifact_key.clone())
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        let mut stream = client
            .get_object()
            .bucket(bucket)
            .key(artifact_key)
            .send()
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .body;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|err| Status::internal(err.to_string()))?;

            tx.send(Ok(RegistryPullResponse {
                data: chunk.to_vec(),
            }))
            .await
            .map_err(|err| Status::internal(format!("failed to send store chunk: {:?}", err)))?;
        }

        Ok(())
    }

    async fn push(&self, metadata: PushMetadata) -> Result<(), Status> {
        let PushMetadata {
            data,
            data_kind,
            hash,
            manifest,
            name,
        } = metadata;

        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_key = get_artifact_key(data_kind, &hash, &name)?;

        let artifact_head_result = client
            .head_object()
            .bucket(bucket)
            .key(&artifact_key)
            .send()
            .await;

        if artifact_head_result.is_ok() {
            return Ok(());
        }

        let _ = client
            .put_object()
            .bucket(bucket)
            .key(artifact_key)
            .body(data.into())
            .send()
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        let manifest_key = get_manifest_key(data_kind, &hash, &name)?;

        let manifest_head_result = client
            .head_object()
            .bucket(bucket)
            .key(&manifest_key)
            .send()
            .await;

        if manifest_head_result.is_ok() {
            return Ok(());
        }

        let manifest_bytes = manifest.into_bytes();

        let _ = client
            .put_object()
            .bucket(bucket)
            .key(manifest_key)
            .body(manifest_bytes.into())
            .send()
            .await
            .map_err(|err| Status::internal(format!("failed to write manifest: {:?}", err)))?;

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn RegistryBackend> {
        Box::new(self.clone())
    }
}
