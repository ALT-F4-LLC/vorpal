use crate::api::config_service_server::ConfigService;
use crate::api::{ConfigPackageRequest, ConfigPackageResponse};
use crate::service::agent::package;
use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct Agent {}

#[tonic::async_trait]
impl ConfigService for Agent {
    type PackageStream = ReceiverStream<Result<ConfigPackageResponse, Status>>;

    async fn package(
        &self,
        request: Request<ConfigPackageRequest>,
    ) -> Result<Response<Self::PackageStream>, Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move { package::run(&tx, request).await });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
