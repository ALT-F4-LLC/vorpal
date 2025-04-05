use crate::LocalBackend;
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
use vorpal_schema::archive::v0::{
    archive_service_server::{ArchiveService, ArchiveServiceServer},
    ArchivePullRequest, ArchivePullResponse, ArchivePushRequest, ArchiveResponse,
};
use vorpal_store::{notary::get_public_key, paths::get_public_key_path};

pub mod gha;
pub mod local;
pub mod s3;

const DEFAULT_GRPC_CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB

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

pub async fn listen(port: u16) -> Result<()> {
    let public_key = get_public_key_path();

    if !public_key.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal keys generate' or copy from agent"
        ));
    }

    let addr = format!("[::]:{}", port)
        .parse()
        .map_err(|err| anyhow::anyhow!("failed to parse address: {:?}", err))?;

    let service = ArchiveServiceServer::new(ArchiveServer::new(Box::new(LocalBackend)));

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        .map_err(|err| anyhow::anyhow!("failed to serve: {:?}", err))?;

    Ok(())
}
