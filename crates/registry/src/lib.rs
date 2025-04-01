use anyhow::Result;
use rsa::{
    pss::{Signature, VerifyingKey},
    sha2::Sha256,
    signature::Verifier,
};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{transport::Server, Request, Response, Status, Streaming};
use tracing::error;
use vorpal_notary::get_public_key;
use vorpal_schema::{
    config::v0::{ConfigArtifact, ConfigArtifactRequest},
    registry::v0::{
        registry_service_server::{RegistryService, RegistryServiceServer},
        RegistryArchive,
        RegistryArchive::UnknownArchive,
        RegistryGetResponse, RegistryPullRequest, RegistryPullResponse, RegistryPushRequest,
        RegistryPushResponse, RegistryPutResponse,
    },
};
use vorpal_store::paths::get_public_key_path;

pub mod gha;
pub mod local;
pub mod s3;
pub use gha::GhaRegistryBackend;
pub use local::LocalRegistryBackend;
pub use s3::S3RegistryBackend;

#[derive(thiserror::Error, Debug)]
pub enum RegistryError {
    #[error("missing s3 bucket")]
    MissingS3Bucket,

    #[error("failed to create GHA cache client: {0}")]
    FailedToCreateGhaClient(String),
}

const DEFAULT_GRPC_CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RegistryServerBackend {
    #[default]
    Unknown,
    GHA,
    Local,
    S3,
}

#[tonic::async_trait]
pub trait RegistryBackend: Send + Sync + 'static {
    async fn get_archive(&self, req: &RegistryPullRequest) -> Result<(), Status>;

    async fn get_artifact(&self, req: &ConfigArtifactRequest) -> Result<ConfigArtifact, Status>;

    async fn pull_archive(
        &self,
        req: &RegistryPullRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status>;

    async fn push_archive(&self, req: &RegistryPushRequest) -> Result<(), Status>;

    async fn put_artifact(&self, req: &ConfigArtifact) -> Result<(), Status>;

    /// Return a new `Box<dyn RegistryBackend>` cloned from `self`.
    fn box_clone(&self) -> Box<dyn RegistryBackend>;
}

impl Clone for Box<dyn RegistryBackend> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

pub struct RegistryServer {
    pub backend: Box<dyn RegistryBackend>,
}

impl RegistryServer {
    pub fn new(backend: Box<dyn RegistryBackend>) -> Self {
        Self { backend }
    }
}

#[tonic::async_trait]
impl RegistryService for RegistryServer {
    type PullArchiveStream = ReceiverStream<Result<RegistryPullResponse, Status>>;

    async fn get_archive(
        &self,
        request: Request<RegistryPullRequest>,
    ) -> Result<Response<RegistryGetResponse>, Status> {
        let req = request.into_inner();

        if req.hash.is_empty() {
            return Err(Status::invalid_argument("missing store id"));
        }

        self.backend.get_archive(&req).await?;

        Ok(Response::new(RegistryGetResponse {}))
    }

    async fn get_config_artifact(
        &self,
        request: Request<ConfigArtifactRequest>,
    ) -> Result<Response<ConfigArtifact>, Status> {
        let req = request.into_inner();

        if req.hash.is_empty() {
            return Err(Status::invalid_argument("missing store id"));
        }

        let artifact = self.backend.get_artifact(&req).await?;

        Ok(Response::new(artifact))
    }

    async fn pull_archive(
        &self,
        request: Request<RegistryPullRequest>,
    ) -> Result<Response<Self::PullArchiveStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        let backend = self.backend.clone();

        tokio::spawn(async move {
            let request = request.into_inner();

            if request.hash.is_empty() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("missing artifact id")))
                    .await
                {
                    error!("failed to send store error: {:?}", err);
                }

                return;
            }

            if let Err(err) = backend.pull_archive(&request, tx.clone()).await {
                if let Err(err) = tx.send(Err(err)).await {
                    error!("failed to send store error: {:?}", err);
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn push_archive(
        &self,
        request: Request<Streaming<RegistryPushRequest>>,
    ) -> Result<Response<RegistryPushResponse>, Status> {
        let mut request_archive = UnknownArchive;
        let mut request_data: Vec<u8> = vec![];
        let mut request_hash = None;
        let mut request_signature = vec![];
        let mut request_stream = request.into_inner();

        while let Some(request) = request_stream.next().await {
            let request = request.map_err(|err| Status::internal(err.to_string()))?;

            request_data.extend_from_slice(&request.data);

            request_archive = RegistryArchive::try_from(request.archive).unwrap_or(UnknownArchive);
            request_hash = Some(request.hash);
            request_signature = request.signature;
        }

        if request_data.is_empty() {
            return Err(Status::invalid_argument("missing `data` field"));
        }

        let Some(request_hash) = request_hash else {
            return Err(Status::invalid_argument("missing `hash` field"));
        };

        if request_archive == UnknownArchive {
            return Err(Status::invalid_argument("missing `kind` field"));
        }

        if request_signature.is_empty() {
            return Err(Status::invalid_argument("missing `data_signature` field"));
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

        let request = RegistryPushRequest {
            archive: request_archive as i32,
            hash: request_hash,
            data: request_data,
            signature: request_signature,
        };

        self.backend.push_archive(&request).await?;

        Ok(Response::new(RegistryPushResponse {}))
    }

    async fn put_config_artifact(
        &self,
        request: Request<ConfigArtifact>,
    ) -> Result<Response<RegistryPutResponse>, Status> {
        let request = request.into_inner();

        self.backend.put_artifact(&request).await?;

        Ok(Response::new(RegistryPutResponse {}))
    }
}

pub async fn listen(port: u16) -> Result<()> {
    let public_key_path = get_public_key_path();

    if !public_key_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let addr = format!("[::]:{}", port)
        .parse()
        .map_err(|err| anyhow::anyhow!("failed to parse address: {:?}", err))?;

    let registry_service =
        RegistryServiceServer::new(RegistryServer::new(Box::new(LocalRegistryBackend)));

    Server::builder()
        .add_service(registry_service)
        .serve(addr)
        .await
        .map_err(|err| anyhow::anyhow!("failed to serve: {:?}", err))?;

    Ok(())
}
