use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use vorpal_schema::vorpal::registry::v0::{RegistryKind, RegistryPullResponse, RegistryRequest};
use vorpal_store::paths::{get_artifact_archive_path, get_source_archive_path, set_timestamps};

use crate::{PushMetadata, RegistryBackend, DEFAULT_GRPC_CHUNK_SIZE};

#[derive(Clone, Debug)]
pub struct LocalRegistryBackend;

#[async_trait]
impl RegistryBackend for LocalRegistryBackend {
    async fn exists(&self, request: &RegistryRequest) -> Result<(), Status> {
        let path = match request.kind() {
            RegistryKind::Artifact => {
                vorpal_store::paths::get_artifact_archive_path(&request.hash, &request.name)
            }
            RegistryKind::ArtifactSource => {
                vorpal_store::paths::get_source_archive_path(&request.hash, &request.name)
            }
            _ => return Err(Status::invalid_argument("unsupported store kind")),
        };

        if !path.exists() {
            return Err(Status::not_found("store path not found"));
        }

        Ok(())
    }

    async fn pull(
        &self,
        request: &RegistryRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status> {
        let path = match request.kind() {
            RegistryKind::Artifact => get_artifact_archive_path(&request.hash, &request.name),
            RegistryKind::ArtifactSource => get_source_archive_path(&request.hash, &request.name),
            _ => return Err(Status::invalid_argument("unsupported store kind")),
        };

        if !path.exists() {
            return Err(Status::not_found("store path not found"));
        }

        let data = read(&path)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        for chunk in data.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
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
            data_kind,
            hash,
            name,
            data,
        } = metadata;

        let path = match data_kind {
            RegistryKind::Artifact => get_artifact_archive_path(&hash, &name),
            RegistryKind::ArtifactSource => get_source_archive_path(&hash, &name),
            _ => return Err(Status::invalid_argument("unsupported store kind")),
        };

        if path.exists() {
            return Ok(());
        }

        write(&path, &data)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        set_timestamps(&path)
            .await
            .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn RegistryBackend> {
        Box::new(self.clone())
    }
}
