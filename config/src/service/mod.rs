use crate::cli::BuildConfigFn;
use anyhow::Result;
use tonic::transport::Server;
use tracing::info;
use vorpal_schema::vorpal::{
    config::v0::{
        config_service_server::{ConfigService, ConfigServiceServer},
        EvaluateRequest, EvaluateResponse,
    },
    package::v0::PackageSystem,
};

#[derive(Debug)]
pub struct ConfigServer {
    pub config: BuildConfigFn,
}

impl ConfigServer {
    pub fn new(config: BuildConfigFn) -> Self {
        Self { config }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServer {
    async fn evaluate(
        &self,
        request: tonic::Request<EvaluateRequest>,
    ) -> Result<tonic::Response<EvaluateResponse>, tonic::Status> {
        let request = request.into_inner();

        info!("received evaluate request: {:?}", request);

        let system = PackageSystem::try_from(request.system)
            .map_err(|e| tonic::Status::invalid_argument(format!("invalid system: {}", e)))?;

        let config = (self.config)(system);

        let response = EvaluateResponse {
            config: Some(config),
        };

        Ok(tonic::Response::new(response))
    }
}

pub async fn listen(config: BuildConfigFn, port: u16) -> Result<()> {
    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    info!("config address: {}", addr);

    Server::builder()
        .add_service(ConfigServiceServer::new(ConfigServer::new(config)))
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}
