use crate::api::package_service_server::PackageService;
use crate::api::{
    BuildRequest, BuildResponse, PrepareRequest, PrepareResponse, RetrieveRequest,
    RetrieveResponse, Status as BuildStatus, StatusRequest, StatusResponse,
};
use anyhow::Result;
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct Packager {}

#[tonic::async_trait]
impl PackageService for Packager {
    async fn prepare(
        &self,
        request: Request<PrepareRequest>,
    ) -> Result<Response<PrepareResponse>, Status> {
        let message = request.into_inner();

        println!(
            "[PackagePrepare]: name={:?} hash={:?}",
            message.source_name, message.source_hash
        );

        // TODO: decompress source_data into store

        let source_id = Uuid::now_v7();

        // TODO: store source in database

        let response = PrepareResponse {
            source_id: source_id.to_string(),
        };

        Ok(Response::new(response))
    }

    async fn build(
        &self,
        request: Request<BuildRequest>,
    ) -> Result<Response<BuildResponse>, Status> {
        println!("[PackageBuild]: {:?}", request);

        let response = BuildResponse {
            build_id: "456".to_string(),
        };

        Ok(Response::new(response))
    }

    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        println!("[PackageStatus]: {:?}", request);

        let response = StatusResponse {
            logs: vec!["log1".to_string(), "log2".to_string()],
            status: BuildStatus::Created.into(),
        };

        Ok(Response::new(response))
    }

    async fn retrieve(
        &self,
        request: Request<RetrieveRequest>,
    ) -> Result<Response<RetrieveResponse>, Status> {
        println!("[PackageRetrieve]: {:?}", request);

        let response = RetrieveResponse { data: Vec::new() };

        Ok(Response::new(response))
    }
}
