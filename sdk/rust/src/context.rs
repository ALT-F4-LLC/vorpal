use crate::cli::{Cli, Command};
use anyhow::Result;
use clap::Parser;
use sha256::digest;
use std::collections::HashMap;
use tonic::transport::Server;
use vorpal_schema::{
    config::v0::{
        config_service_server::{ConfigService, ConfigServiceServer},
        Config, ConfigArtifact, ConfigArtifactRequest, ConfigArtifactSystem, ConfigRequest,
    },
    system_from_str,
};

const _DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit

#[derive(Clone, Debug, Default)]
pub struct ConfigContext {
    artifact: HashMap<String, ConfigArtifact>,
    port: u16,
    // registry: String,
    system: ConfigArtifactSystem,
}

#[derive(Debug, Default)]
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

            Ok(ConfigContext::new(port, registry, target))
        }
    }
}

impl ConfigContext {
    pub fn new(port: u16, _registry: String, system: ConfigArtifactSystem) -> Self {
        Self {
            artifact: HashMap::new(),
            port,
            // registry,
            system,
        }
    }

    pub fn add_artifact(&mut self, config: ConfigArtifact) -> Result<String> {
        // 1. Calculate hash

        let config_json = serde_json::to_string(&config).map_err(|e| anyhow::anyhow!(e))?;
        let config_hash = digest(config_json.as_bytes());

        // 2. Insert context

        self.artifact.insert(config_hash.clone(), config);

        // 3. Return id

        Ok(config_hash)
    }

    // pub async fn fetch_artifact(&mut self, _hash: &str) -> Result<String> {
    //     let registry_host = self.registry.clone();
    //
    //     let mut registry = RegistryServiceClient::connect(registry_host.to_owned())
    //         .await
    //         .expect("failed to connect to registry");
    //
    //     let registry_request = ConfigArtifactRequest {
    //         hash: hash.to_string(),
    //     };
    //
    //     match registry.get_config_artifact(registry_request.clone()).await {
    //         Err(status) => {
    //             if status.code() != NotFound {
    //                 bail!("registry get config artifact error: {:?}", status);
    //             }
    //
    //             bail!("config for artifact not found: {:?}", registry_request);
    //         }
    //
    //         Ok(response) => {
    //             let config = response.into_inner();
    //
    //             self.artifact.insert(hash.to_string(), config);
    //
    //             Ok(hash.to_string())
    //         }
    //     }
    //
    //     Ok("TODO".to_string())
    // }

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
