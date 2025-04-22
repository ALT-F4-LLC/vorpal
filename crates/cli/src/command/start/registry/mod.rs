use crate::command::store::{notary::get_public_key, paths::get_public_key_path};
use anyhow::Result;
use aws_sdk_s3::Client;
use rsa::{
    pss::{Signature, VerifyingKey},
    sha2::Sha256,
    signature::Verifier,
};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::error;
use vorpal_sdk::api::archive::{
    archive_service_server::ArchiveService, ArchivePullRequest, ArchivePullResponse,
    ArchivePushRequest, ArchiveResponse,
};
use vorpal_sdk::api::artifact::{
    artifact_service_server::ArtifactService, Artifact, ArtifactRequest, ArtifactResponse,
    ArtifactsRequest, ArtifactsResponse,
};

mod archive;
mod artifact;
mod gha;
mod s3;

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("missing s3 bucket")]
    MissingS3Bucket,

    #[error("failed to create GHA cache client: {0}")]
    FailedToCreateGhaClient(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ServerBackend {
    #[default]
    Unknown,
    GHA,
    Local,
    S3,
}

#[derive(Clone, Debug)]
pub struct LocalBackend;

#[derive(Debug, Clone)]
pub struct GhaBackend {
    cache_client: gha::CacheClient,
}

const DEFAULT_GRPC_CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB

impl GhaBackend {
    pub fn new() -> Result<Self, BackendError> {
        let cache_client = gha::CacheClient::new()
            .map_err(|err| BackendError::FailedToCreateGhaClient(err.to_string()))?;

        Ok(Self { cache_client })
    }
}

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

        let client_version = aws_config::BehaviorVersion::v2025_01_17();
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
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn push(
        &self,
        request: Request<Streaming<ArchivePushRequest>>,
    ) -> Result<Response<ArchiveResponse>, Status> {
        let mut request_data: Vec<u8> = vec![];
        let mut request_digest = None;
        let mut request_signature = vec![];
        let mut request_stream = request.into_inner();

        while let Some(request) = request_stream.next().await {
            let request = request.map_err(|err| Status::internal(err.to_string()))?;

            request_data.extend_from_slice(&request.data);

            request_digest = Some(request.digest);
            request_signature = request.signature;
        }

        if request_data.is_empty() {
            return Err(Status::invalid_argument("missing `data` field"));
        }

        let Some(request_digest) = request_digest else {
            return Err(Status::invalid_argument("missing `digest` field"));
        };

        if request_signature.is_empty() {
            return Err(Status::invalid_argument("missing `signature` field"));
        }

        let public_key_path = get_public_key_path();

        let public_key = get_public_key(public_key_path).await.map_err(|err| {
            Status::internal(format!("failed to get public key: {:?}", err.to_string()))
        })?;

        let data_signature = Signature::try_from(request_signature.as_slice())
            .map_err(|err| Status::internal(format!("failed to parse signature: {:?}", err)))?;

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        if let Err(msg) = verifying_key.verify(&request_data, &data_signature) {
            return Err(Status::invalid_argument(format!(
                "invalid data signature: {:?}",
                msg
            )));
        }

        let request = ArchivePushRequest {
            digest: request_digest,
            data: request_data,
            signature: request_signature,
        };

        self.backend.push(&request).await?;

        Ok(Response::new(ArchiveResponse {}))
    }

    // async fn put_config_artifact(
    //     &self,
    //     request: Request<Artifact>,
    // ) -> Result<Response<RegistryPutResponse>, Status> {
    //     let request = request.into_inner();
    //
    //     self.backend.put_artifact(&request).await?;
    //
    //     Ok(Response::new(RegistryPutResponse {}))
    // }
}

#[tonic::async_trait]
pub trait ArtifactBackend: Send + Sync + 'static {
    async fn get_artifact(&self, artifact_digest: String) -> Result<Artifact, Status>;
    async fn store_artifact(&self, artifact: &Artifact) -> Result<String, Status>;

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

        let artifact = self.backend.get_artifact(request.digest).await?;

        Ok(Response::new(artifact))
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
        request: Request<Artifact>,
    ) -> Result<Response<ArtifactResponse>, Status> {
        let request = request.into_inner();

        let digest = self.backend.store_artifact(&request).await?;

        Ok(Response::new(ArtifactResponse { digest }))
    }
}

pub async fn get_archive_backend(
    registry_backend: String,
    registry_backend_s3_bucket: Option<String>,
) -> Result<Box<dyn ArchiveBackend>> {
    let backend = match registry_backend.as_str() {
        "gha" => ServerBackend::GHA,
        "local" => ServerBackend::Local,
        "s3" => ServerBackend::S3,
        _ => ServerBackend::Unknown,
    };

    let backend_archive: Box<dyn ArchiveBackend> = match backend {
        ServerBackend::Local => Box::new(LocalBackend::new()?),
        ServerBackend::S3 => Box::new(S3Backend::new(registry_backend_s3_bucket.clone()).await?),
        ServerBackend::GHA => Box::new(GhaBackend::new()?),
        ServerBackend::Unknown => unreachable!(),
    };

    Ok(backend_archive)
}

pub async fn get_artifact_backend(
    registry_backend: &str,
    registry_backend_s3_bucket: Option<String>,
) -> Result<Box<dyn ArtifactBackend>> {
    let backend = match registry_backend {
        "gha" => ServerBackend::GHA,
        "local" => ServerBackend::Local,
        "s3" => ServerBackend::S3,
        _ => ServerBackend::Unknown,
    };

    let backend_artifact: Box<dyn ArtifactBackend> = match backend {
        ServerBackend::Local => Box::new(LocalBackend::new()?),
        ServerBackend::S3 => Box::new(S3Backend::new(registry_backend_s3_bucket.clone()).await?),
        ServerBackend::GHA => Box::new(GhaBackend::new()?),
        ServerBackend::Unknown => unreachable!(),
    };

    Ok(backend_artifact)
}
