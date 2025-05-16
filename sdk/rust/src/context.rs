use crate::{
    api::{
        agent::{agent_service_client::AgentServiceClient, PrepareArtifactRequest},
        artifact::{
            artifact_service_client::ArtifactServiceClient,
            artifact_service_server::{ArtifactService, ArtifactServiceServer},
            Artifact, ArtifactRequest, ArtifactResponse, ArtifactSystem, ArtifactsRequest,
            ArtifactsResponse, GetArtifactAliasRequest, GetArtifactAliasResponse,
            StoreArtifactRequest,
        },
    },
    cli::{Cli, Command},
    system::get_system,
};
use anyhow::{bail, Result};
use clap::Parser;
use sha256::digest;
use std::{collections::HashMap, path::PathBuf};
use tonic::{transport::Server, Code::NotFound, Request, Response, Status};
use tracing::info;

#[derive(Clone)]
pub struct ConfigContextStore {
    artifact: HashMap<String, Artifact>,
    variable: HashMap<String, String>,
}

#[derive(Clone)]
pub struct ConfigContext {
    agent: String,
    artifact: String,
    artifact_context: PathBuf,
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

    async fn get_artifact_alias(
        &self,
        _request: Request<GetArtifactAliasRequest>,
    ) -> Result<Response<GetArtifactAliasResponse>, Status> {
        Err(Status::unimplemented("not implemented yet"))
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
        _request: Request<StoreArtifactRequest>,
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
            artifact_context,
            port,
            registry,
            system,
            variable,
        } => Ok(ConfigContext::new(
            agent,
            artifact,
            PathBuf::from(artifact_context),
            port,
            registry,
            system,
            variable,
        )?),
    }
}

impl ConfigContext {
    pub fn new(
        agent: String,
        artifact: String,
        artifact_context: PathBuf,
        port: u16,
        registry: String,
        system: String,
        variable: Vec<String>,
    ) -> Result<Self> {
        Ok(Self {
            agent,
            artifact,
            artifact_context,
            port,
            registry,
            store: ConfigContextStore {
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
            },
            system: get_system(&system)?,
        })
    }

    pub async fn add_artifact(&mut self, artifact: &Artifact) -> Result<String> {
        if artifact.name.is_empty() {
            bail!("name cannot be empty");
        }

        if artifact.steps.is_empty() {
            bail!("steps cannot be empty");
        }

        if artifact.systems.is_empty() {
            bail!("systems cannot be empty");
        }

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

        let request = PrepareArtifactRequest {
            artifact: Some(artifact.clone()),
            artifact_context: self.artifact_context.display().to_string(),
        };

        let response = client
            .prepare_artifact(request)
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

    pub async fn fetch_artifact(&mut self, alias: &str) -> Result<String> {
        // TODO: look in lockfile for artifact version

        // if self.store.artifact.contains_key(digest) {
        //     return Ok(digest.to_string());
        // }

        let mut client = ArtifactServiceClient::connect(self.registry.clone())
            .await
            .expect("failed to connect to artifact service");

        let request = GetArtifactAliasRequest {
            alias: alias.to_string(),
            alias_system: self.system.into(),
        };

        match client.get_artifact_alias(request.clone()).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("artifact service error: {:?}", status);
                }

                bail!("artifact alias not found: {alias}");
            }

            Ok(response) => {
                let response = response.into_inner();

                let request = ArtifactRequest {
                    digest: response.digest,
                };

                match client.get_artifact(request.clone()).await {
                    Err(status) => {
                        if status.code() != NotFound {
                            bail!("artifact service error: {:?}", status);
                        }

                        bail!("artifact not found: {}", request.digest);
                    }

                    Ok(response) => {
                        let artifact = response.into_inner();

                        self.store
                            .artifact
                            .insert(request.digest.clone(), artifact.clone());

                        for step in artifact.steps.iter() {
                            for artifact_digest in step.artifacts.iter() {
                                Box::pin(self.fetch_artifact(artifact_digest)).await?;
                            }
                        }

                        Ok(request.digest)
                    }
                }
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

    pub fn get_system(&self) -> ArtifactSystem {
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
