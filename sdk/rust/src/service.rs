use crate::context::ConfigContext;
use anyhow::Result;
use vorpal_schema::vorpal::{
    artifact::v0::{Artifact, ArtifactId},
    config::v0::{config_service_server::ConfigService, Config, ConfigRequest},
};

#[derive(Debug, Default)]
pub struct ConfigServer {
    pub context: ConfigContext,
    pub config: Config,
}

impl ConfigServer {
    pub fn new(context: ConfigContext, config: Config) -> Self {
        Self { context, config }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServer {
    async fn get_config(
        &self,
        _request: tonic::Request<ConfigRequest>,
    ) -> Result<tonic::Response<Config>, tonic::Status> {
        Ok(tonic::Response::new(self.config.clone()))
    }

    async fn get_artifact(
        &self,
        request: tonic::Request<ArtifactId>,
    ) -> Result<tonic::Response<Artifact>, tonic::Status> {
        let request = request.into_inner();

        let artifact = self
            .context
            .get_artifact(request.hash.as_str(), request.name.as_str());

        if artifact.is_none() {
            return Err(tonic::Status::not_found("Artifact input not found"));
        }

        Ok(tonic::Response::new(artifact.unwrap().clone()))
    }
}
