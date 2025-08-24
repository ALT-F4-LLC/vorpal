use crate::{
    api::{
        agent::{agent_service_client::AgentServiceClient, PrepareArtifactRequest},
        artifact::{
            artifact_service_client::ArtifactServiceClient, Artifact, ArtifactRequest,
            ArtifactSystem, ArtifactsRequest, ArtifactsResponse, GetArtifactAliasRequest,
        },
        context::context_service_server::{ContextService, ContextServiceServer},
    },
    artifact::system::get_system,
    cli::{Cli, Command},
};
use anyhow::{bail, Result};
use clap::Parser;
use http::uri::{InvalidUri, Uri};
use sha256::digest;
use std::{collections::HashMap, env, path::PathBuf};
use tokio::fs::read;
use tonic::{
    transport::{Certificate, Channel, ClientTlsConfig, Server},
    Code::NotFound,
    Request, Response, Status,
};
use tracing::info;

#[derive(Clone)]
pub struct ConfigContextStore {
    artifact: HashMap<String, Artifact>,
    variable: HashMap<String, String>,
}

#[derive(Clone)]
pub struct ConfigContext {
    artifact: String,
    artifact_context: PathBuf,
    client_agent: AgentServiceClient<Channel>,
    client_api_token: String,
    client_artifact: ArtifactServiceClient<Channel>,
    port: u16,
    store: ConfigContextStore,
    system: ArtifactSystem,
    unlock: bool,
}

#[derive(Clone)]
pub struct ConfigServer {
    pub store: ConfigContextStore,
}

impl ConfigServer {
    pub fn new(store: ConfigContextStore) -> Self {
        Self { store }
    }
}

#[tonic::async_trait]
impl ContextService for ConfigServer {
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
            unlock,
        } => {
            let service_ca_pem = read("/var/lib/vorpal/key/ca.pem")
                .await
                .expect("failed to read CA certificate");

            let service_ca = Certificate::from_pem(service_ca_pem);

            let service_tls = ClientTlsConfig::new()
                .ca_certificate(service_ca)
                .domain_name("localhost");

            let client_agent_uri = agent
                .parse::<Uri>()
                .map_err(|e: InvalidUri| anyhow::anyhow!("invalid agent address: {}", e))?;

            let client_agent_channel = Channel::builder(client_agent_uri)
                .tls_config(service_tls.clone())?
                .connect()
                .await?;

            let client_registry_uri = registry
                .parse::<Uri>()
                .map_err(|e: InvalidUri| anyhow::anyhow!("invalid artifact address: {}", e))?;

            let client_registry_channel = Channel::builder(client_registry_uri)
                .tls_config(service_tls)?
                .connect()
                .await?;

            let client_agent = AgentServiceClient::new(client_agent_channel);
            let client_artifact = ArtifactServiceClient::new(client_registry_channel);

            let client_api_token = env::var("VORPAL_API_TOKEN").unwrap_or_default();

            if client_api_token.is_empty() {
                bail!("VORPAL_API_TOKEN environment variable is required");
            }

            Ok(ConfigContext::new(
                artifact,
                PathBuf::from(artifact_context),
                client_agent,
                client_api_token,
                client_artifact,
                port,
                system,
                unlock,
                variable,
            )?)
        }
    }
}

impl ConfigContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        artifact: String,
        artifact_context: PathBuf,
        client_agent: AgentServiceClient<Channel>,
        client_api_token: String,
        client_artifact: ArtifactServiceClient<Channel>,
        port: u16,
        system: String,
        unlock: bool,
        variable: Vec<String>,
    ) -> Result<Self> {
        Ok(Self {
            client_api_token,
            artifact,
            artifact_context,
            client_agent,
            client_artifact,
            port,
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
            unlock,
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

        // Send raw sources to agent - agent will handle all lockfile operations
        let artifact_json =
            serde_json::to_vec(&artifact).expect("failed to serialize artifact to JSON");

        let artifact_digest = digest(artifact_json);

        if self.store.artifact.contains_key(&artifact_digest) {
            return Ok(artifact_digest);
        }

        // TODO: make this run in parallel

        let request = PrepareArtifactRequest {
            artifact: Some(artifact.clone()),
            artifact_context: self.artifact_context.display().to_string(),
            artifact_unlock: self.unlock,
        };

        let mut request = Request::new(request);

        request.metadata_mut().insert(
            "authorization",
            self.client_api_token
                .parse()
                .expect("failed to set authorization header"),
        );

        let response = self
            .client_agent
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

        self.store
            .artifact
            .insert(artifact_digest.clone(), artifact.clone());

        Ok(artifact_digest)
    }

    pub async fn fetch_artifact(&mut self, alias: &str) -> Result<String> {
        // TODO: look in lockfile for artifact version

        // if self.store.artifact.contains_key(digest) {
        //     return Ok(digest.to_string());
        // }

        fn is_hex_digest(s: &str) -> bool {
            s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
        }

        // Treat 64-hex strings as direct digests; otherwise resolve via alias
        if is_hex_digest(alias) {
            let request = ArtifactRequest {
                digest: alias.to_string(),
            };

            let mut grpc_request = Request::new(request.clone());

            grpc_request.metadata_mut().insert(
                "authorization",
                self.client_api_token
                    .parse()
                    .expect("failed to set authorization header"),
            );

            match self.client_artifact.get_artifact(grpc_request).await {
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
                        for dep in step.artifacts.iter() {
                            Box::pin(self.fetch_artifact(dep)).await?;
                        }
                    }

                    return Ok(request.digest);
                }
            }
        }

        let request = GetArtifactAliasRequest {
            alias: alias.to_string(),
            alias_system: self.system.into(),
        };

        let mut grpc_request = Request::new(request.clone());

        grpc_request.metadata_mut().insert(
            "authorization",
            self.client_api_token
                .parse()
                .expect("failed to set authorization header"),
        );

        match self.client_artifact.get_artifact_alias(grpc_request).await {
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

                let mut grpc_request = Request::new(request.clone());

                grpc_request.metadata_mut().insert(
                    "authorization",
                    self.client_api_token
                        .parse()
                        .expect("failed to set authorization header"),
                );

                match self.client_artifact.get_artifact(grpc_request).await {
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

    pub fn get_artifact_context_path(&self) -> &PathBuf {
        &self.artifact_context
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
        let service = ContextServiceServer::new(ConfigServer::new(self.store.clone()));

        let service_addr_str = format!("[::]:{}", self.port);
        let service_addr = service_addr_str.parse().expect("failed to parse address");

        println!("context service: {service_addr_str}");

        Server::builder()
            .add_service(service)
            .serve(service_addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))
    }
}
