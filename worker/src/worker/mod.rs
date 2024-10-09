use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use vorpal_schema::vorpal::{
    package::v0::PackageSystem,
    worker::v0::{worker_service_server::WorkerService, BuildRequest, BuildResponse},
};

mod build;
mod darwin;
mod linux;
mod native;

#[derive(Debug, Default)]
pub struct WorkerServer {
    pub system: PackageSystem,
}

impl WorkerServer {
    pub fn new(system: PackageSystem) -> Self {
        Self { system }
    }
}

#[tonic::async_trait]
impl WorkerService for WorkerServer {
    type BuildStream = ReceiverStream<Result<BuildResponse, Status>>;

    async fn build(
        &self,
        request: Request<Streaming<BuildRequest>>,
    ) -> Result<Response<Self::BuildStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        // TODO: create build environment directory

        tokio::spawn(async move {
            match build::run(request, &tx).await {
                Ok(_) => (),
                Err(e) => {
                    // TODO: also clean up all files in the sandbox if they exists

                    tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
