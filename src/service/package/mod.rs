use crate::api::package_service_server::PackageService;
use crate::api::{
    PackageBuildRequest, PackageBuildResponse, PackageBuildSystem, PackagePrepareRequest,
    PackagePrepareResponse,
};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

mod build;
mod prepare;
mod sandbox_default;

#[derive(Debug, Default)]
pub struct Package {
    pub system: PackageBuildSystem,
}

impl Package {
    pub fn new(system: PackageBuildSystem) -> Self {
        Self { system }
    }
}

#[tonic::async_trait]
impl PackageService for Package {
    type BuildStream = ReceiverStream<Result<PackageBuildResponse, Status>>;
    type PrepareStream = ReceiverStream<Result<PackagePrepareResponse, Status>>;

    async fn prepare(
        &self,
        request: Request<Streaming<PackagePrepareRequest>>,
    ) -> Result<Response<Self::PrepareStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move { prepare::run(&tx, request).await });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn build(
        &self,
        request: Request<PackageBuildRequest>,
    ) -> Result<Response<Self::BuildStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move { build::run(&tx, request).await });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
