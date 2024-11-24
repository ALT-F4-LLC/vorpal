use anyhow::Result;
use sha256::digest;
use std::collections::HashMap;
use tonic::transport::Server;
use vorpal_schema::vorpal::{
    artifact::v0::{Artifact, ArtifactId, ArtifactSystem},
    config::v0::{
        config_service_server::{ConfigService, ConfigServiceServer},
        Config, ConfigRequest,
    },
};

#[derive(Debug, Default)]
pub struct ContextConfig {
    artifact: HashMap<String, Artifact>,
    target: ArtifactSystem,
}

impl ContextConfig {
    pub fn new(target: ArtifactSystem) -> Self {
        Self {
            artifact: HashMap::new(),
            target,
        }
    }

    pub fn add_artifact(&mut self, artifact: Artifact) -> Result<ArtifactId> {
        let artifact_json = serde_json::to_string(&artifact).map_err(|e| anyhow::anyhow!(e))?;
        let artifact_hash = digest(artifact_json.as_bytes());
        let artifact_key = format!("{}-{}", artifact.name, artifact_hash);

        if !self.artifact.contains_key(&artifact_key) {
            self.artifact.insert(artifact_key.clone(), artifact.clone());
        }

        let artifact_id = ArtifactId {
            hash: artifact_hash,
            name: artifact.name,
        };

        Ok(artifact_id)
    }

    pub fn get_artifact(&self, hash: &str, name: &str) -> Option<&Artifact> {
        let artifact_key = format!("{}-{}", name, hash);

        self.artifact.get(&artifact_key)
    }

    pub fn get_target(&self) -> ArtifactSystem {
        self.target
    }
}

#[derive(Debug, Default)]
pub struct ConfigServer {
    pub context: ContextConfig,
    pub config: Config,
}

impl ConfigServer {
    pub fn new(context: ContextConfig, config: Config) -> Self {
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

pub async fn listen(context: ContextConfig, config: Config, port: u16) -> Result<()> {
    let addr = format!("[::]:{}", port)
        .parse()
        .expect("failed to parse address");

    let config_service = ConfigServiceServer::new(ConfigServer::new(context, config));

    println!("Config server listening on {}", addr);

    Server::builder()
        .add_service(config_service)
        .serve(addr)
        .await
        .expect("failed to start worker server");

    Ok(())
}
