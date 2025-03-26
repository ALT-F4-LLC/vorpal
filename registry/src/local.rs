use crate::{PushMetadata, RegistryBackend, RegistryError, DEFAULT_GRPC_CHUNK_SIZE};
use tokio::{
    fs::{read, write},
    sync::mpsc,
};
use tonic::{async_trait, Status};
use vorpal_schema::vorpal::registry::v0::{RegistryKind, RegistryPullResponse, RegistryRequest};
use vorpal_store::paths::{
    get_artifact_archive_path, get_artifact_manifest_path, get_source_archive_path,
    get_source_manifest_path, set_timestamps,
};

#[derive(Clone, Debug)]
pub struct LocalRegistryBackend;

impl LocalRegistryBackend {
    pub fn new() -> Result<Self, RegistryError> {
        Ok(Self)
    }
}

fn get_archive_path(
    kind: RegistryKind,
    hash: &str,
    name: &str,
) -> Result<std::path::PathBuf, Status> {
    match kind {
        RegistryKind::Artifact => Ok(get_artifact_archive_path(hash, name)),
        RegistryKind::ArtifactSource => Ok(get_source_archive_path(hash, name)),
        _ => Err(Status::invalid_argument("unsupported store kind")),
    }
}

fn get_manifest_path(
    kind: RegistryKind,
    hash: &str,
    name: &str,
) -> Result<std::path::PathBuf, Status> {
    match kind {
        RegistryKind::Artifact => Ok(get_artifact_manifest_path(hash, name)),
        RegistryKind::ArtifactSource => Ok(get_source_manifest_path(hash, name)),
        _ => Err(Status::invalid_argument("unsupported store kind")),
    }
}

#[async_trait]
impl RegistryBackend for LocalRegistryBackend {
    async fn exists(&self, request: &RegistryRequest) -> Result<String, Status> {
        let manifest_path = get_manifest_path(request.kind(), &request.hash, &request.name)?;

        if !manifest_path.exists() {
            return Err(Status::not_found("store manifest not found"));
        }

        let manifest_data = read(&manifest_path)
            .await
            .map_err(|err| Status::internal(format!("failed to read manifest: {:?}", err)))?;

        let manifest = String::from_utf8(manifest_data)
            .map_err(|err| Status::internal(format!("failed to parse manifest: {:?}", err)))?;

        Ok(manifest)
    }

    async fn pull(
        &self,
        request: &RegistryRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status> {
        let archive_path = get_archive_path(request.kind(), &request.hash, &request.name)?;

        if !archive_path.exists() {
            return Err(Status::not_found("store path not found"));
        }

        let archive_data = read(&archive_path)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;

        for chunk in archive_data.chunks(DEFAULT_GRPC_CHUNK_SIZE) {
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

        let archive_path = get_archive_path(data_kind, &hash, &name)?;

        write(&archive_path, &data)
            .await
            .map_err(|err| Status::internal(format!("failed to write store path: {:?}", err)))?;

        set_timestamps(&archive_path)
            .await
            .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;

        let manifest_path = get_manifest_path(data_kind, &hash, &name)?;

        write(&manifest_path, &manifest)
            .await
            .map_err(|err| Status::internal(format!("failed to write manifest: {:?}", err)))?;

        set_timestamps(&manifest_path)
            .await
            .map_err(|err| Status::internal(format!("failed to sanitize path: {:?}", err)))?;

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn RegistryBackend> {
        Box::new(self.clone())
    }
}
