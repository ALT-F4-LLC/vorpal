use crate::cli::{Cli, Command};
use anyhow::{bail, Result};
use clap::Parser;
use std::collections::HashMap;
use tonic::{
    transport::{Channel, Server},
    Code::NotFound,
    Request, Response, Status,
};
use vorpal_schema::{
    agent::v0::agent_service_client::AgentServiceClient,
    artifact::v0::{
        artifact_service_client::ArtifactServiceClient,
        artifact_service_server::{ArtifactService, ArtifactServiceServer},
        Artifact, ArtifactRequest, ArtifactResponse, ArtifactStep, ArtifactSystem,
        ArtifactsRequest, ArtifactsResponse,
    },
    system_from_str,
};

#[derive(Clone)]
pub struct ConfigContext {
    agent: String,
    artifact: HashMap<String, Artifact>,
    port: u16,
    registry: String,
    system: ArtifactSystem,
}

#[derive(Clone)]
pub struct ArtifactServer {
    pub artifacts: Vec<String>,
    pub context: ConfigContext,
}

impl ArtifactServer {
    pub fn new(artifacts: Vec<String>, context: ConfigContext) -> Self {
        Self { artifacts, context }
    }
}

#[tonic::async_trait]
impl ArtifactService for ArtifactServer {
    async fn get_artifact(
        &self,
        request: Request<ArtifactRequest>,
    ) -> Result<Response<Artifact>, Status> {
        let request = request.into_inner();
        let request_artifact = self.context.get_artifact(&request.digest);

        if request_artifact.is_none() {
            return Err(tonic::Status::not_found("artifact not found"));
        }

        Ok(Response::new(request_artifact.unwrap().clone()))
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
            ..
        } => {
            let target = system_from_str(&target)?;

            Ok(ConfigContext::new(agent, port, registry, target))
        }
    }
}

impl ConfigContext {
    pub fn new(agent: String, port: u16, registry: String, system: ArtifactSystem) -> Self {
        Self {
            agent,
            artifact: HashMap::new(),
            port,
            registry,
            system,
        }
    }

    pub async fn add_artifact(&mut self, artifact: Artifact) -> Result<String> {
        // 1. Prepare artifact

        let mut client = AgentServiceClient::connect(self.agent.clone())
            .await
            .expect("failed to connect to agent service");

        let response = client
            .prepare_artifact(artifact.clone())
            .await
            .expect("failed to prepare artifact");

        let response = response.into_inner();

        if response.artifact.is_none() {
            bail!("artifact not returned from agent service");
        }

        // 2. Insert context

        self.artifact
            .insert(response.artifact_digest.clone(), response.artifact.unwrap());

        // 3. Return digest

        Ok(response.artifact_digest)
    }

    async fn fetch_step_artifacts(
        &mut self,
        artifact_client: &mut ArtifactServiceClient<Channel>,
        artifact_steps: Vec<ArtifactStep>,
    ) -> Result<()> {
        for step in artifact_steps.iter() {
            for artifact_digest in step.artifacts.iter() {
                if self.artifact.contains_key(artifact_digest) {
                    continue;
                }

                let request = ArtifactRequest {
                    digest: artifact_digest.to_string(),
                };

                let response = match artifact_client.get_artifact(request).await {
                    Ok(res) => res,
                    Err(error) => {
                        if error.code() != NotFound {
                            bail!("artifact service error: {:?}", error);
                        }

                        bail!("artifact not found: {artifact_digest}");
                    }
                };

                let artifact = response.into_inner();

                self.artifact
                    .insert(artifact_digest.to_string(), artifact.clone());

                Box::pin(self.fetch_step_artifacts(artifact_client, artifact.steps)).await?
            }
        }

        Ok(())
    }

    pub async fn fetch_artifact(&mut self, digest: &str) -> Result<String> {
        if self.artifact.contains_key(digest) {
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

                self.fetch_step_artifacts(&mut client, artifact.steps.clone())
                    .await?;

                self.artifact.insert(digest.to_string(), artifact);

                Ok(digest.to_string())
            }
        }
    }

    pub fn get_artifact(&self, hash: &str) -> Option<Artifact> {
        self.artifact.get(hash).cloned()
    }

    pub fn get_target(&self) -> ArtifactSystem {
        self.system
    }

    pub async fn run(&self, artifacts: Vec<String>) -> Result<()> {
        let addr = format!("[::]:{}", self.port)
            .parse()
            .expect("failed to parse address");

        let service = ArtifactServiceServer::new(ArtifactServer::new(artifacts, self.clone()));

        Server::builder()
            .add_service(service)
            .serve(addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))
    }
}
