use anyhow::Result;
use tonic::transport::Server;
use tracing::info;
use vorpal_schema::vorpal::config::v0::{
    config_service_server::{ConfigService, ConfigServiceServer},
    Config, EvaluateRequest, EvaluateResponse,
};

#[derive(Debug, Default)]
pub struct ConfigServer {
    pub config: Config,
}

impl ConfigServer {
    pub fn new(config: Config) -> Self {
        Self { config }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServer {
    async fn evaluate(
        &self,
        request: tonic::Request<EvaluateRequest>,
    ) -> Result<tonic::Response<EvaluateResponse>, tonic::Status> {
        info!("received config: {:?}", request.get_ref());

        let response = EvaluateResponse {
            config: Some(self.config.clone()),
        };

        Ok(tonic::Response::new(response))
    }
}

pub async fn listen(config: Config, port: u16) -> Result<()> {
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
