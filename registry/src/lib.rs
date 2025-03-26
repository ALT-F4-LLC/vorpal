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
use vorpal_schema::vorpal::registry::v0::{
    registry_service_server::{RegistryService, RegistryServiceServer},
    RegistryKind::{self, UnknownStoreKind},
    RegistryPullResponse, RegistryPushRequest, RegistryRequest, RegistryResponse,
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

pub struct PushMetadata {
    data: Vec<u8>,
    data_kind: RegistryKind,
    hash: String,
    manifest: String,
    name: String,
}

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
    async fn exists(&self, request: &RegistryRequest) -> Result<String, Status>;

    async fn pull(
        &self,
        request: &RegistryRequest,
        tx: mpsc::Sender<Result<RegistryPullResponse, Status>>,
    ) -> Result<(), Status>;

    async fn push(&self, metadata: PushMetadata) -> Result<(), Status>;

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
    type PullStream = ReceiverStream<Result<RegistryPullResponse, Status>>;

    async fn exists(
        &self,
        request: Request<RegistryRequest>,
    ) -> Result<Response<RegistryResponse>, Status> {
        let request = request.into_inner();

        if request.hash.is_empty() {
            return Err(Status::invalid_argument("missing store id"));
        }

        if request.name.is_empty() {
            return Err(Status::invalid_argument("missing store name"));
        }

        let manifest = self.backend.exists(&request).await?;

        Ok(Response::new(RegistryResponse { manifest }))
    }

    async fn pull(
        &self,
        request: Request<RegistryRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
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
        request: Request<Streaming<RegistryPushRequest>>,
    ) -> Result<Response<RegistryResponse>, Status> {
        let mut data: Vec<u8> = vec![];
        let mut data_hash = None;
        let mut data_kind = UnknownStoreKind;
        let mut data_manifest = None;
        let mut data_name = None;
        let mut data_signature = vec![];
        let mut stream = request.into_inner();

        while let Some(result) = stream.next().await {
            let result = result.map_err(|err| Status::internal(err.to_string()))?;

            data.extend_from_slice(&result.data);

            data_hash = Some(result.hash);
            data_kind = RegistryKind::try_from(result.kind).unwrap_or(UnknownStoreKind);
            data_manifest = Some(result.manifest);
            data_name = Some(result.name);
            data_signature = result.data_signature;
        }

        if data.is_empty() {
            return Err(Status::invalid_argument("missing `data` field"));
        }

        let Some(data_hash) = data_hash else {
            return Err(Status::invalid_argument("missing `hash` field"));
        };

        let Some(data_manifest) = data_manifest else {
            return Err(Status::invalid_argument("missing `manifest` field"));
        };

        let Some(data_name) = data_name else {
            return Err(Status::invalid_argument("missing `name` field"));
        };

        if data_kind == UnknownStoreKind {
            return Err(Status::invalid_argument("missing `kind` field"));
        }

        if data_signature.is_empty() {
            return Err(Status::invalid_argument("missing `data_signature` field"));
        }

        let public_key_path = get_public_key_path();

        let public_key = get_public_key(public_key_path).await.map_err(|err| {
            Status::internal(format!("failed to get public key: {:?}", err.to_string()))
        })?;

        let data_signature = Signature::try_from(data_signature.as_slice())
            .map_err(|err| Status::internal(format!("failed to parse signature: {:?}", err)))?;

        let verifying_key = VerifyingKey::<Sha256>::new(public_key);

        if let Err(msg) = verifying_key.verify(&data, &data_signature) {
            return Err(Status::invalid_argument(format!(
                "invalid data signature: {:?}",
                msg
            )));
        }

        let hash = data_hash;
        let manifest = data_manifest;
        let name = data_name;

        self.backend
            .push(PushMetadata {
                data,
                data_kind,
                hash,
                manifest: manifest.clone(),
                name,
            })
            .await?;

        Ok(Response::new(RegistryResponse { manifest }))
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
