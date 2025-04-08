use crate::{
    cli::{Cli, Command},
    system::get_system,
};
use anyhow::{bail, Result};
use clap::Parser;
use sha256::digest;
use std::collections::HashMap;
use tonic::{transport::Server, Code::NotFound, Request, Response, Status};
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
pub struct ConfigContext {
    agent: String,
    port: u16,
    registry: String,
    store: HashMap<String, Artifact>,
    system: ArtifactSystem,
}

#[derive(Clone)]
pub struct ArtifactServer {
    pub artifacts: Vec<String>,
    pub store: HashMap<String, Artifact>,
}

impl ArtifactServer {
    pub fn new(artifacts: Vec<String>, store: HashMap<String, Artifact>) -> Self {
        Self { artifacts, store }
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

        let artifact = self.store.get(request.digest.as_str());

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
            digests: self.artifacts.clone(),
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
            port,
            registry,
            target,
        } => {
            let target = get_system(&target)?;

            Ok(ConfigContext::new(agent, port, registry, target))
        }
    }
}

impl ConfigContext {
    pub fn new(agent: String, port: u16, registry: String, system: ArtifactSystem) -> Self {
        Self {
            agent,
            port,
            registry,
            store: HashMap::new(),
            system,
        }
    }

    pub async fn add_artifact(&mut self, artifact: Artifact) -> Result<String> {
        let artifact_json =
            serde_json::to_string(&artifact).expect("failed to serialize artifact to JSON");
        let artifact_digest = digest(artifact_json);

        if self.store.contains_key(artifact_digest.as_str()) {
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
                        println!("{} |> {}", artifact.name, artifact_output);
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

        if !self.store.contains_key(artifact_digest.as_str()) {
            self.store.insert(artifact_digest.clone(), artifact.clone());
        }

        Ok(artifact_digest)
    }

    pub async fn fetch_artifact(&mut self, digest: &str) -> Result<String> {
        if self.store.contains_key(digest) {
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

                self.store.insert(digest.to_string(), artifact.clone());

                for step in artifact.steps.iter() {
                    for artifact_digest in step.artifacts.iter() {
                        Box::pin(self.fetch_artifact(artifact_digest)).await?;
                    }
                }

                Ok(digest.to_string())
            }
        }
    }

    pub fn get_artifact(&self, digest: &str) -> Option<Artifact> {
        self.store.get(digest).cloned()
    }

    pub fn get_target(&self) -> ArtifactSystem {
        self.system
    }

    pub async fn run(&self, artifacts: Vec<String>) -> Result<()> {
        let addr = format!("[::]:{}", self.port)
            .parse()
            .expect("failed to parse address");

        let service =
            ArtifactServiceServer::new(ArtifactServer::new(artifacts, self.store.clone()));

        println!("artifact service: {}", addr);

        Server::builder()
            .add_service(service)
            .serve(addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))
    }
}
