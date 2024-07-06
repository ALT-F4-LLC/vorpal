use crate::api::config_service_server::ConfigService;
use crate::api::ConfigPackageBuildSystem;
use crate::api::{ConfigPackageOutput, ConfigPackageRequest, ConfigPackageResponse};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio_stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::debug;

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
                Err(e) => return send_error(&tx, e.to_string()).await,
                Ok(_) => Ok(()),
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
async fn send_error(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    message: String,
) -> Result<(), anyhow::Error> {
    debug!("send_error: {}", message);

    tx.send(Err(Status::internal(message.clone()))).await?;

    anyhow::bail!(message);
}

async fn send(
    tx: &Sender<Result<ConfigPackageResponse, Status>>,
    log_output: String,
    package_output: Option<ConfigPackageOutput>,
) -> Result<(), anyhow::Error> {
    debug!("send: {:?}", log_output);

    tx.send(Ok(ConfigPackageResponse {
        log_output,
        package_output,
    }))
    .await?;

    Ok(())
}
