use crate::LocalBackend;
use anyhow::Result;
use tonic::{transport::Server, Request, Response, Status};
use vorpal_schema::artifact::v0::{
    artifact_service_server::{ArtifactService, ArtifactServiceServer},
    Artifact, ArtifactRequest, ArtifactResponse, ArtifactsRequest, ArtifactsResponse,
};

pub mod gha;
pub mod local;
pub mod s3;

#[tonic::async_trait]
pub trait ArtifactBackend: Send + Sync + 'static {
    async fn get_artifact(&self, req: &ArtifactRequest) -> Result<Artifact, Status>;
    async fn store_artifact(&self, req: &Artifact) -> Result<String, Status>;

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
        let req = request.into_inner();

        if req.digest.is_empty() {
            return Err(Status::invalid_argument("missing `digest` field"));
        }

        let artifact = self.backend.get_artifact(&req).await?;

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

pub async fn listen(port: u16) -> Result<()> {
    let addr = format!("[::]:{}", port)
        .parse()
        .map_err(|err| anyhow::anyhow!("failed to parse address: {:?}", err))?;

    let service = ArtifactServiceServer::new(ArtifactServer::new(Box::new(LocalBackend)));

    Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        .map_err(|err| anyhow::anyhow!("failed to serve: {:?}", err))?;

    Ok(())
}
