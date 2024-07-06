use crate::api::config_service_server::ConfigService;
use crate::api::ConfigPackageBuildSystem;
use crate::api::{ConfigPackageRequest, ConfigPackageResponse};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

mod source;
mod stream;

#[derive(Clone, Debug)]
pub struct ConfigWorker {
    pub system: ConfigPackageBuildSystem,
    pub uri: String,
}

#[derive(Debug, Default)]
pub struct Config {
    pub workers: Vec<ConfigWorker>,
}

impl Config {
    pub fn new(workers: &Vec<ConfigWorker>) -> Self {
        Self {
            workers: workers.to_vec(),
        }
    }
}

#[tonic::async_trait]
impl ConfigService for Config {
    type PackageStream = ReceiverStream<Result<ConfigPackageResponse, Status>>;

    async fn package(
        &self,
        request: Request<ConfigPackageRequest>,
    ) -> Result<Response<Self::PackageStream>, Status> {
        let (tx, rx) = mpsc::channel(100);

        let workers = self.workers.clone();

        tokio::spawn(async move {
            match stream::package(&tx, request, workers).await {
                Ok(_) => (),
                Err(e) => {
                    tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
