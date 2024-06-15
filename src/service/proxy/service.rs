use crate::api::command_service_server::CommandService;
use crate::api::{PackageRequest, PackageResponse};
use crate::service::proxy::package;
use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct Proxy {}

#[tonic::async_trait]
impl CommandService for Proxy {
    type PackageStream = ReceiverStream<Result<PackageResponse, Status>>;

    async fn package(
        &self,
        request: Request<PackageRequest>,
    ) -> Result<Response<Self::PackageStream>, Status> {
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            let req = request.into_inner();

            let req_source = req
                .source
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("source is required"))?;

            tx.send(Ok(PackageResponse {
                package_log: format!("preparing package: {}", req.name),
            }))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

            let (source_id, source_hash) = package::prepare(&tx, &req.name, req_source)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            tx.send(Ok(PackageResponse {
                package_log: format!("building package: {}-{}", req.name, source_hash),
            }))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

            package::build(&tx, source_id, &source_hash, &req)
                .await
                .map_err(|e| Status::internal(format!("Failed to build package: {}", e)))
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
