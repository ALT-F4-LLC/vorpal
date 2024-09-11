use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use vorpal_schema::api::package::{
    package_service_server::PackageService, BuildRequest, BuildResponse, PackageSystem,
};

mod build;
mod darwin;
mod linux;
mod native;

#[derive(Debug, Default)]
pub struct PackageServer {
    pub system: PackageSystem,
}

impl PackageServer {
    pub fn new(system: PackageSystem) -> Self {
        Self { system }
    }
}

#[tonic::async_trait]
impl PackageService for PackageServer {
    type BuildStream = ReceiverStream<Result<BuildResponse, Status>>;

    async fn build(
        &self,
        request: Request<Streaming<BuildRequest>>,
    ) -> Result<Response<Self::BuildStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            match build::run(request, &tx).await {
                Ok(_) => (),
                Err(e) => {
                    tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
