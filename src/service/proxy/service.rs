use crate::api::cli_service_server::CliService;
use crate::api::{PackageRequest, PackageResponse};
use crate::service::proxy::package;
use anyhow::Result;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct Proxy {}

#[tonic::async_trait]
impl CliService for Proxy {
    async fn package(
        &self,
        request: Request<PackageRequest>,
    ) -> Result<Response<PackageResponse>, Status> {
        package::run(request).await
    }
}
