use anyhow::{bail, Result};
use aws_sdk_s3::Client;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info};
use vorpal_sdk::api::{
    archive::{
        archive_service_server::ArchiveService, ArchivePullRequest, ArchivePullResponse,
        ArchivePushRequest, ArchiveResponse,
    },
    artifact::{
        artifact_service_server::ArtifactService, Artifact, ArtifactRequest, ArtifactResponse,
        ArtifactSystem, ArtifactsRequest, ArtifactsResponse, GetArtifactAliasRequest,
        GetArtifactAliasResponse, StoreArtifactRequest,
    },
};

mod archive;
mod artifact;
mod s3;

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("missing s3 bucket")]
    MissingS3Bucket,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ServerBackend {
    #[default]
    Unknown,
    Local,
    S3,
}

#[derive(Clone, Debug)]
pub struct LocalBackend;

const DEFAULT_GRPC_CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB

#[derive(Clone, Debug)]
pub struct S3Backend {
    bucket: String,
    client: Client,
}

impl LocalBackend {
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self)
    }
}

impl S3Backend {
    pub async fn new(bucket: Option<String>) -> Result<Self, BackendError> {
        let Some(bucket) = bucket else {
            return Err(BackendError::MissingS3Bucket);
        };

        let client_version = aws_config::BehaviorVersion::v2025_08_07();
        let client_config = aws_config::load_defaults(client_version).await;
        let client = Client::new(&client_config);

        Ok(Self { bucket, client })
    }
}

#[tonic::async_trait]
pub trait ArchiveBackend: Send + Sync + 'static {
    async fn check(&self, req: &ArchivePullRequest) -> Result<(), Status>;

    async fn pull(
        &self,
        req: &ArchivePullRequest,
        tx: mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status>;

    async fn push(&self, req: &ArchivePushRequest) -> Result<(), Status>;

    /// Return a new `Box<dyn RegistryBackend>` cloned from `self`.
    fn box_clone(&self) -> Box<dyn ArchiveBackend>;
}

impl Clone for Box<dyn ArchiveBackend> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

pub struct ArchiveServer {
    pub backend: Box<dyn ArchiveBackend>,
}

impl ArchiveServer {
    pub fn new(backend: Box<dyn ArchiveBackend>) -> Self {
        Self { backend }
    }
}

#[tonic::async_trait]
impl ArchiveService for ArchiveServer {
    type PullStream = ReceiverStream<Result<ArchivePullResponse, Status>>;

    async fn check(
        &self,
        request: Request<ArchivePullRequest>,
    ) -> Result<Response<ArchiveResponse>, Status> {
        let req = request.into_inner();

        if req.digest.is_empty() {
            return Err(Status::invalid_argument("missing `digest` field"));
        }

        self.backend.check(&req).await?;

        info!("registry |> archive check: {}", req.digest);

        Ok(Response::new(ArchiveResponse {}))
    }

    async fn pull(
        &self,
        request: Request<ArchivePullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        let backend = self.backend.clone();

        tokio::spawn(async move {
            let request = request.into_inner();

            if request.digest.is_empty() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("missing `digest` field")))
                    .await
                {
                    error!("failed to send store error: {:?}", err);
                }

                return;
            }

            if let Err(err) = backend.pull(&request, tx.clone()).await {
                if let Err(err) = tx.send(Err(err)).await {
                    error!("failed to send store error: {:?}", err);
                }
            }

            info!("registry |> archive pull: {}", request.digest);
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn push(
        &self,
        request: Request<Streaming<ArchivePushRequest>>,
    ) -> Result<Response<ArchiveResponse>, Status> {
        let mut request_data: Vec<u8> = vec![];
        let mut request_digest = None;
        let mut request_namespace = None;
        let mut request_stream = request.into_inner();

        while let Some(request) = request_stream.next().await {
            let request = request.map_err(|err| Status::internal(err.to_string()))?;

            request_data.extend_from_slice(&request.data);

            request_digest = Some(request.digest);
            request_namespace = Some(request.namespace);
        }

        if request_data.is_empty() {
            return Err(Status::invalid_argument("missing `data` field"));
        }

        let Some(request_digest) = request_digest else {
            return Err(Status::invalid_argument("missing `digest` field"));
        };

        let Some(request_namespace) = request_namespace else {
            return Err(Status::invalid_argument("missing `namespace` field"));
        };

        let request = ArchivePushRequest {
            digest: request_digest,
            data: request_data,
            namespace: request_namespace,
        };

        self.backend.push(&request).await?;

        info!("registry |> archive push: {}", request.digest);

        Ok(Response::new(ArchiveResponse {}))
    }
}

#[tonic::async_trait]
pub trait ArtifactBackend: Send + Sync + 'static {
    async fn get_artifact(&self, digest: String, namespace: String) -> Result<Artifact, Status>;

    async fn get_artifact_alias(
        &self,
        artifact_name: String,
        artifact_namespace: String,
        artifact_system: ArtifactSystem,
        artifact_tag: String,
    ) -> Result<String, Status>;

    async fn store_artifact(
        &self,
        artifact: Artifact,
        artifact_aliases: Vec<String>,
        artifact_namespace: String,
    ) -> Result<String, Status>;

    /// Return a new `Box<dyn RegistryBackend>` cloned from `self`.
    fn box_clone(&self) -> Box<dyn ArtifactBackend>;
}

impl Clone for Box<dyn ArtifactBackend> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

pub struct ArtifactServer {
    pub backend: Box<dyn ArtifactBackend>,
}

impl ArtifactServer {
    pub fn new(backend: Box<dyn ArtifactBackend>) -> Self {
        Self { backend }
    }
}

#[tonic::async_trait]
impl ArtifactService for ArtifactServer {
    async fn get_artifact(
        &self,
        request: Request<ArtifactRequest>,
    ) -> Result<Response<Artifact>, Status> {
        let request = request.into_inner();

        if request.digest.is_empty() {
            return Err(Status::invalid_argument("missing `digest` field"));
        }

        let artifact = self
            .backend
            .get_artifact(request.digest.clone(), request.namespace.clone())
            .await?;

        info!("artifact |> get: {}", request.digest);

        Ok(Response::new(artifact))
    }

    async fn get_artifact_alias(
        &self,
        request: Request<GetArtifactAliasRequest>,
    ) -> Result<Response<GetArtifactAliasResponse>, Status> {
        let request = request.into_inner();

        let request_system = ArtifactSystem::try_from(request.system);

        let digest = self
            .backend
            .get_artifact_alias(
                request.name.clone(),
                request.namespace,
                request_system.unwrap_or(ArtifactSystem::UnknownSystem),
                request.tag.clone(),
            )
            .await?;

        info!(
            "artifact |> alias get: {}:{} -> {}",
            request.name, request.tag, digest
        );

        Ok(Response::new(GetArtifactAliasResponse { digest }))
    }

    async fn get_artifacts(
        &self,
        _request: Request<ArtifactsRequest>,
    ) -> Result<Response<ArtifactsResponse>, Status> {
        // TODO: implement this method
        // let request = request.into_inner();
        // let digests = self.backend.get_artifacts(&request).await?;
        // Ok(Response::new(ArtifactsResponse { digests }))
        Err(Status::unimplemented(
            "get_artifacts is not implemented yet",
        ))
    }

    async fn store_artifact(
        &self,
        request: Request<StoreArtifactRequest>,
    ) -> Result<Response<ArtifactResponse>, Status> {
        let request = request.into_inner();

        let artifact = request
            .artifact
            .ok_or_else(|| Status::invalid_argument("missing `artifact` field"))?;

        let digest = self
            .backend
            .store_artifact(
                artifact,
                request.artifact_aliases,
                request.artifact_namespace,
            )
            .await?;

        info!("artifact |> store: {}", digest);

        Ok(Response::new(ArtifactResponse { digest }))
    }
}

pub async fn backend_archive(
    registry_backend: String,
    registry_backend_s3_bucket: Option<String>,
) -> Result<Box<dyn ArchiveBackend>> {
    let backend = match registry_backend.as_str() {
        "local" => ServerBackend::Local,
        "s3" => ServerBackend::S3,
        _ => ServerBackend::Unknown,
    };

    let backend_archive: Box<dyn ArchiveBackend> = match backend {
        ServerBackend::Local => Box::new(LocalBackend::new()?),
        ServerBackend::S3 => Box::new(S3Backend::new(registry_backend_s3_bucket.clone()).await?),
        ServerBackend::Unknown => bail!("unknown archive backend: {}", registry_backend),
    };

    Ok(backend_archive)
}

pub async fn backend_artifact(
    registry_backend: &str,
    registry_backend_s3_bucket: Option<String>,
) -> Result<Box<dyn ArtifactBackend>> {
    let backend = match registry_backend {
        "local" => ServerBackend::Local,
        "s3" => ServerBackend::S3,
        _ => ServerBackend::Unknown,
    };

    let backend_artifact: Box<dyn ArtifactBackend> = match backend {
        ServerBackend::Local => Box::new(LocalBackend::new()?),
        ServerBackend::S3 => Box::new(S3Backend::new(registry_backend_s3_bucket.clone()).await?),
        ServerBackend::Unknown => bail!("unknown artifact backend: {}", registry_backend),
    };

    Ok(backend_artifact)
}
