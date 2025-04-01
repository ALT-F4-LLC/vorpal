use crate::cli::{Cli, Command};
use anyhow::{bail, Result};
use clap::Parser;
use sha256::digest;
use std::collections::HashMap;
use tonic::{transport::Server, Code::NotFound};
use vorpal_schema::{
    config::v0::{
        config_service_server::{ConfigService, ConfigServiceServer},
        Config, ConfigArtifact, ConfigArtifactRequest, ConfigArtifactSystem, ConfigRequest,
    },
    registry::v0::registry_service_client::RegistryServiceClient,
    system_from_str,
};

#[derive(Clone, Debug)]
pub struct ConfigContext {
    artifact: HashMap<String, ConfigArtifact>,
    port: u16,
    registry: RegistryServiceClient<tonic::transport::Channel>,
    system: ConfigArtifactSystem,
}

#[derive(Debug)]
pub struct ConfigServer {
    pub config: Config,
    pub context: ConfigContext,
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

    async fn get_config_artifact(
        &self,
        request: tonic::Request<ConfigArtifactRequest>,
    ) -> Result<tonic::Response<ConfigArtifact>, tonic::Status> {
        let request = request.into_inner();

        let config = self.context.get_artifact(&request.hash);

        if config.is_none() {
            return Err(tonic::Status::not_found("config for artifact not found"));
        }

        Ok(tonic::Response::new(config.unwrap().clone()))
    }
}

pub async fn get_context() -> Result<ConfigContext> {
    let args = Cli::parse();

    match args.command {
        Command::Start {
            port,
            registry,
            target,
            ..
        } => {
            let target = system_from_str(&target)?;

            let registry = RegistryServiceClient::connect(registry.clone())
                .await
                .expect("failed to connect to registry");

            Ok(ConfigContext::new(port, registry, target))
        }
    }
}

impl ConfigContext {
    pub fn new(
        port: u16,
        registry: RegistryServiceClient<tonic::transport::Channel>,
        system: ConfigArtifactSystem,
    ) -> Self {
        Self {
            artifact: HashMap::new(),
            port,
            registry,
            system,
        }
    }

    pub fn add_artifact(&mut self, config: ConfigArtifact) -> Result<String> {
        // 1. Calculate hash

        let artifact_json = serde_json::to_string(&config).map_err(|e| anyhow::anyhow!(e))?;
        let artifact_hash = digest(artifact_json.as_bytes());

        // 2. Insert context

        self.artifact.insert(artifact_hash.clone(), config);

        // 3. Return id

        Ok(artifact_hash)
    }

    pub async fn fetch_artifact(&mut self, hash: &str) -> Result<String> {
        if self.artifact.contains_key(hash) {
            return Ok(hash.to_string());
        }

        let request = ConfigArtifactRequest {
            hash: hash.to_string(),
        };

        match self.registry.get_config_artifact(request.clone()).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("registry get config artifact error: {:?}", status);
                }

                bail!("config artifact not found: {hash}");
            }

            Ok(response) => {
                let config = response.into_inner();

                self.artifact.insert(hash.to_string(), config);

                Ok(hash.to_string())
            }
        }
    }

    pub fn get_artifact(&self, hash: &str) -> Option<ConfigArtifact> {
        self.artifact.get(hash).cloned()
    }

    pub fn get_target(&self) -> ConfigArtifactSystem {
        self.system.clone()
    }

    pub async fn run(&self, artifacts: Vec<String>) -> Result<()> {
        let addr = format!("[::]:{}", self.port)
            .parse()
            .expect("failed to parse address");

        let config = Config { artifacts };

        let context = self.clone();

        let config_service = ConfigServiceServer::new(ConfigServer::new(context, config));

        println!("Config listening: {}", addr);

        Server::builder()
            .add_service(config_service)
            .serve(addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))
    }
}
