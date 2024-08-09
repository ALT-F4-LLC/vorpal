use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use vorpal_schema::api::package::{
    package_service_server::PackageService, BuildRequest, BuildResponse, PackageSystem,
};

mod build;
mod prepare;

#[derive(Debug, Default)]
pub struct PackageServer {
    pub target: PackageSystem,
}

impl PackageServer {
    pub fn new(target: PackageSystem) -> Self {
        Self { target }
    }
}

#[tonic::async_trait]
impl PackageService for PackageServer {
    type BuildStream = ReceiverStream<Result<BuildResponse, Status>>;
    // type PrepareStream = ReceiverStream<Result<PrepareResponse, Status>>;

    // async fn prepare(
    //     &self,
    //     request: Request<Streaming<PrepareRequest>>,
    // ) -> Result<Response<Self::PrepareStream>, Status> {
    //     let (tx, rx) = mpsc::channel(100);
    //
    //     tokio::spawn(async move {
    //         match prepare::run(&tx, request).await {
    //             Ok(_) => (),
    //             Err(e) => {
    //                 tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
    //             }
    //         }
    //     });
    //
    //     Ok(Response::new(ReceiverStream::new(rx)))
    // }

    async fn build(
        &self,
        request: Request<Streaming<BuildRequest>>,
    ) -> Result<Response<Self::BuildStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            match build::run(&tx, request).await {
                Ok(_) => (),
                Err(e) => {
                    tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
