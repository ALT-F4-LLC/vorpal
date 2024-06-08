use crate::api::package_service_server::PackageService;
use crate::api::{BuildRequest, BuildResponse, PrepareRequest, PrepareResponse};
use crate::service::build::{build, prepare};
use anyhow::Result;
use tonic::{Request, Response, Status, Streaming};

#[derive(Debug, Default)]
pub struct Package {}

#[tonic::async_trait]
impl PackageService for Package {
    async fn prepare(
        &self,
        request: Request<Streaming<PrepareRequest>>,
    ) -> Result<Response<PrepareResponse>, Status> {
        prepare::run(request.into_inner()).await
    }

    async fn build(
        &self,
        request: Request<BuildRequest>,
    ) -> Result<Response<BuildResponse>, Status> {
        build::run(request).await
    }
}
