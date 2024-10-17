use anyhow::Result;
// use tokio::fs::remove_dir_all;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use vorpal_schema::vorpal::{
    package::v0::PackageSystem,
    worker::v0::{worker_service_server::WorkerService, BuildRequest, BuildResponse},
};
use vorpal_store::temps::create_temp_dir;

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

        tokio::spawn(async move {
            let build_path = match create_temp_dir().await {
                Ok(path) => path,
                Err(e) => {
                    tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                    return;
                }
            };

            if let Err(e) = build::run(&build_path, request, &tx).await {
                tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            }

            // if let Err(e) = remove_dir_all(build_path.clone()).await {
            //     tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            // }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
