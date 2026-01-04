use crate::{
    api::{
        agent::{agent_service_client::AgentServiceClient, PrepareArtifactRequest},
        artifact::{
            artifact_service_client::ArtifactServiceClient, Artifact, ArtifactFunction,
            ArtifactFunctionRequest, ArtifactFunctionsRequest, ArtifactRequest, ArtifactSystem,
            ArtifactsRequest, ArtifactsResponse,
        },
        context::context_service_server::{ContextService, ContextServiceServer},
    },
    artifact::system::get_system,
    cli::{Cli, Command},
};
use anyhow::{anyhow, bail, Result};
use clap::Parser;
use http::uri::{InvalidUri, Uri};
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::fs::read;
use tonic::{
    metadata::{Ascii, MetadataValue},
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
    artifact_namespace: String,
    artifact_system: ArtifactSystem,
    artifact_unlock: bool,
    client_agent: AgentServiceClient<Channel>,
    client_artifact: ArtifactServiceClient<Channel>,
    port: u16,
    registry: String,
    store: ConfigContextStore,
}

#[derive(Clone)]
pub struct ConfigServer {
    pub store: ConfigContextStore,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VorpalCredentialsContent {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VorpalCredentials {
    pub issuer: HashMap<String, VorpalCredentialsContent>,
    pub registry: HashMap<String, String>,
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
            artifact_namespace,
            artifact_system,
            artifact_unlock,
            artifact_variable,
            port,
            registry,
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

            Ok(ConfigContext::new(
                artifact,
                PathBuf::from(artifact_context),
                artifact_namespace,
                artifact_system,
                artifact_unlock,
                artifact_variable,
                client_agent,
                client_artifact,
                port,
                registry,
            )?)
        }
    }
}

impl ConfigContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        artifact: String,
        artifact_context: PathBuf,
        artifact_namespace: String,
        artifact_system: String,
        artifact_unlock: bool,
        artifact_variable: Vec<String>,
        client_agent: AgentServiceClient<Channel>,
        client_artifact: ArtifactServiceClient<Channel>,
        port: u16,
        registry: String,
    ) -> Result<Self> {
        Ok(Self {
            artifact,
            artifact_context,
            client_agent,
            client_artifact,
            artifact_namespace,
            port,
            registry,
            store: ConfigContextStore {
                artifact: HashMap::new(),
                variable: artifact_variable
                    .iter()
                    .map(|v| {
                        let mut parts = v.split('=');
                        let name = parts.next().unwrap_or_default();
                        let value = parts.next().unwrap_or_default();
                        (name.to_string(), value.to_string())
                    })
                    .collect(),
            },
            artifact_system: get_system(&artifact_system)?,
            artifact_unlock,
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
            artifact_namespace: self.artifact_namespace.clone(),
            artifact_unlock: self.artifact_unlock,
            registry: self.registry.clone(),
        };

        let mut request = Request::new(request);
        let request_auth = client_auth_header(&self.registry).await?;

        if let Some(header) = request_auth {
            request.metadata_mut().insert("authorization", header);
        }

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

    pub async fn fetch_artifact(&mut self, digest: &str) -> Result<String> {
        // TODO: look in lockfile for artifact version

        let request = ArtifactRequest {
            digest: digest.to_string(),
            namespace: self.artifact_namespace.clone(),
        };

        let mut request = Request::new(request.clone());
        let request_auth = client_auth_header(&self.registry).await?;

        if let Some(header) = request_auth {
            request.metadata_mut().insert("authorization", header);
        }

        match self.client_artifact.get_artifact(request).await {
            Err(status) => {
                if status.code() != NotFound {
                    bail!("artifact service error: {:?}", status);
                }

                bail!("artifact not found: {}", digest);
            }

            Ok(response) => {
                let artifact = response.into_inner();

                self.store
                    .artifact
                    .insert(digest.to_string(), artifact.clone());

                for step in artifact.steps.iter() {
                    for dep in step.artifacts.iter() {
                        Box::pin(self.fetch_artifact(dep)).await?;
                    }
                }

                Ok(digest.to_string())
            }
        }
    }

    pub async fn get_artifact_functions(
        &mut self,
        namespace: Option<&str>,
        name_prefix: Option<&str>,
    ) -> Result<Vec<ArtifactFunction>> {
        let request = ArtifactFunctionsRequest {
            namespace: namespace.unwrap_or_default().to_string(),
            name_prefix: name_prefix.unwrap_or_default().to_string(),
        };

        let mut request = Request::new(request);
        let request_auth = client_auth_header(&self.registry).await?;

        if let Some(header) = request_auth {
            request.metadata_mut().insert("authorization", header);
        }

        let response = self
            .client_artifact
            .get_artifact_functions(request)
            .await
            .map_err(|status| anyhow!("artifact service error: {:?}", status))?;

        Ok(response.into_inner().functions)
    }

    pub async fn get_artifact_function(
        &mut self,
        name: &str,
        namespace: &str,
        tag: &str,
        params: HashMap<String, String>,
    ) -> Result<Artifact> {
        let tag = if tag.is_empty() { "latest" } else { tag };

        let request = ArtifactFunctionRequest {
            name: name.to_string(),
            namespace: namespace.to_string(),
            tag: tag.to_string(),
            system: self.artifact_system as i32,
            params,
        };

        let mut request = Request::new(request);
        let request_auth = client_auth_header(&self.registry).await?;

        if let Some(header) = request_auth {
            request.metadata_mut().insert("authorization", header);
        }

        let response = self
            .client_artifact
            .get_artifact_function(request)
            .await
            .map_err(|status| anyhow!("artifact service error: {:?}", status))?;

        Ok(response.into_inner())
    }

    pub async fn add_artifact_function(
        &mut self,
        name: &str,
        namespace: &str,
        tag: &str,
        params: HashMap<String, String>,
    ) -> Result<String> {
        let artifact = self
            .get_artifact_function(name, namespace, tag, params)
            .await?;

        self.add_artifact(&artifact).await
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
        self.artifact_system
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

pub fn get_root_dir_path() -> PathBuf {
    Path::new("/var/lib/vorpal").to_path_buf()
}

pub fn get_root_key_dir_path() -> PathBuf {
    get_root_dir_path().join("key")
}

pub fn get_key_credentials_path() -> PathBuf {
    get_root_key_dir_path()
        .join("credentials")
        .with_extension("json")
}

pub async fn client_auth_header(registry: &str) -> Result<Option<MetadataValue<Ascii>>> {
    let mut header: Option<MetadataValue<_>> = None;

    let credentials_path = get_key_credentials_path();

    if credentials_path.exists() {
        let credentials_data = read(credentials_path).await?;

        let credentials: VorpalCredentials = serde_json::from_slice(&credentials_data)?;

        let registry_issuer = credentials
            .registry
            .get(registry)
            .ok_or_else(|| anyhow!("no issuer found for registry: {}", registry))?;

        let issuer_credentials = credentials.issuer.get(registry_issuer).ok_or_else(|| {
            anyhow!(
                "no issuer found for registry: {} (issuer: {})",
                registry,
                registry_issuer
            )
        })?;

        header = Some(
            format!("Bearer {}", issuer_credentials.access_token)
                .parse()
                .map_err(|e| anyhow!("failed to parse service secret: {}", e))?,
        );
    }

    Ok(header)
}
