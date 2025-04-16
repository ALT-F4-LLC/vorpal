use crate::{
    cli::{Cli, Command},
    system::get_system,
};
use anyhow::{bail, Result};
use clap::Parser;
use sha256::digest;
use std::collections::HashMap;
use tonic::{transport::Server, Code::NotFound, Request, Response, Status};
use tracing::info;
use vorpal_schema::{
    agent::v0::agent_service_client::AgentServiceClient,
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient,
        artifact_service_server::{ArtifactService, ArtifactServiceServer},
        Artifact, ArtifactRequest, ArtifactResponse, ArtifactSystem, ArtifactsRequest,
        ArtifactsResponse,
    },
};

#[derive(Clone)]
pub struct ConfigContextStore {
    artifact: HashMap<String, Artifact>,
    variable: HashMap<String, String>,
}

#[derive(Clone)]
pub struct ConfigContext {
    agent: String,
    artifact: String,
    port: u16,
    registry: String,
    store: ConfigContextStore,
    system: ArtifactSystem,
}

#[derive(Clone)]
pub struct ArtifactServer {
    pub store: ConfigContextStore,
}

impl ArtifactServer {
    pub fn new(store: ConfigContextStore) -> Self {
        Self { store }
    }
}

#[tonic::async_trait]
impl ArtifactService for ArtifactServer {
    async fn get_artifact(
        &self,
        request: Request<ArtifactRequest>,
    ) -> Result<Response<Artifact>, Status> {
        let request = request.into_inner();

        if request.digest.is_empty() {
            return Err(tonic::Status::invalid_argument("'digest' is required"));
        }

        let artifact = self.store.artifact.get(request.digest.as_str());

        if artifact.is_none() {
            return Err(tonic::Status::not_found("artifact not found"));
        }

        Ok(Response::new(artifact.unwrap().clone()))
    }

    async fn get_artifacts(
        &self,
        _: tonic::Request<ArtifactsRequest>,
    ) -> Result<tonic::Response<ArtifactsResponse>, tonic::Status> {
        let response = ArtifactsResponse {
            digests: self.store.artifact.keys().cloned().collect(),
        };

        Ok(Response::new(response))
    }

    async fn store_artifact(
        &self,
        _request: Request<Artifact>,
    ) -> Result<Response<ArtifactResponse>, Status> {
        Err(Status::unimplemented("not implemented yet"))
    }
}

pub async fn get_context() -> Result<ConfigContext> {
    let args = Cli::parse();

    match args.command {
        Command::Start {
            agent,
            artifact,
            port,
            registry,
            target,
            variable,
        } => Ok(ConfigContext::new(
            agent, artifact, port, registry, target, variable,
        )?),
    }
}

impl ConfigContext {
    pub fn new(
        agent: String,
        artifact: String,
        port: u16,
        registry: String,
        system: String,
        variable: Vec<String>,
    ) -> Result<Self> {
        let store = ConfigContextStore {
            artifact: HashMap::new(),
            variable: variable
                .iter()
                .map(|v| {
                    let mut parts = v.split('=');
                    let name = parts.next().unwrap_or_default();
                    let value = parts.next().unwrap_or_default();
                    (name.to_string(), value.to_string())
                })
                .collect(),
        };

        let system = get_system(&system)?;

        Ok(Self {
            agent,
            artifact,
            port,
            registry,
            store,
            system,
        })
    }

    pub async fn add_artifact(&mut self, artifact: &Artifact) -> Result<String> {
        let artifact_json =
            serde_json::to_vec(artifact).expect("failed to serialize artifact to JSON");

        let artifact_digest = digest(artifact_json);

        if self.store.artifact.contains_key(&artifact_digest) {
            return Ok(artifact_digest);
        }

        // TODO: make this run in parallel

        let mut client = AgentServiceClient::connect(self.agent.clone())
            .await
            .expect("failed to connect to agent service");

        let response = client
            .prepare_artifact(artifact.clone())
            .await
            .expect("failed to prepare artifact");

        let mut response = response.into_inner();
        let mut response_artifact = None;
        let mut response_artifact_digest = None;

        loop {
            match response.message().await {
                Ok(Some(message)) => {
                    if let Some(artifact_output) = message.artifact_output {
                        if self.port == 0 {
                            info!("{} |> {}", artifact.name, artifact_output);
                        } else {
                            println!("{} |> {}", artifact.name, artifact_output);
                        }
                    }

                    response_artifact = message.artifact;
                    response_artifact_digest = message.artifact_digest;
                }
                Ok(None) => break,
                Err(status) => {
                    if status.code() != NotFound {
                        bail!("{}", status.message());
                    }

                    break;
                }
            }
        }

        if response_artifact.is_none() {
            bail!("artifact not returned from agent service");
        }

        if response_artifact_digest.is_none() {
            bail!("artifact digest not returned from agent service");
        }

        let artifact = response_artifact.unwrap();
        let artifact_digest = response_artifact_digest.unwrap();

        if !self.store.artifact.contains_key(&artifact_digest) {
            self.store
                .artifact
                .insert(artifact_digest.clone(), artifact.clone());
        }

        Ok(artifact_digest)
    }

    pub async fn fetch_artifact(&mut self, digest: &str) -> Result<String> {
        if self.store.artifact.contains_key(digest) {
            return Ok(digest.to_string());
        }

        let mut client = ArtifactServiceClient::connect(self.registry.clone())
            .await
            .expect("failed to connect to artifact service");

        let request = ArtifactRequest {
            digest: digest.to_string(),
        };

        match client.get_artifact(request.clone()).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("artifact service error: {:?}", status);
                }

                bail!("artifact not found: {digest}");
            }

            Ok(response) => {
                let artifact = response.into_inner();

                self.store
                    .artifact
                    .insert(digest.to_string(), artifact.clone());

                for step in artifact.steps.iter() {
                    for artifact_digest in step.artifacts.iter() {
                        Box::pin(self.fetch_artifact(artifact_digest)).await?;
                    }
                }

                Ok(digest.to_string())
            }
        }
    }

    pub fn get_artifact_store(&self) -> HashMap<String, Artifact> {
        self.store.artifact.clone()
    }

    pub fn get_artifact(&self, digest: &str) -> Option<Artifact> {
        self.store.artifact.get(digest).cloned()
    }

    pub fn get_artifact_name(&self) -> &str {
        self.artifact.as_str()
    }

    pub fn get_target(&self) -> ArtifactSystem {
        self.system
    }

    pub fn get_variable(&self, name: &str) -> Option<String> {
        self.store.variable.get(name).cloned()
    }

    pub async fn run(&self) -> Result<()> {
        let service = ArtifactServiceServer::new(ArtifactServer::new(self.store.clone()));

        let service_addr_str = format!("[::]:{}", self.port);
        let service_addr = service_addr_str.parse().expect("failed to parse address");

        println!("artifact service: {}", service_addr_str);

        Server::builder()
            .add_service(service)
            .serve(service_addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))
    }
}
