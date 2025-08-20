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
    lock::{load as load_lockfile, save as save_lockfile, Lockfile},
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
    lock: Option<Lockfile>,
    port: u16,
    registry: String,
    store: ConfigContextStore,
    system: ArtifactSystem,
    update: bool,
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
            update,
        } => Ok(ConfigContext::new(
            agent,
            artifact,
            PathBuf::from(artifact_context),
            port,
            registry,
            system,
            update,
            variable,
        )?),
    }
}

impl ConfigContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent: String,
        artifact: String,
        artifact_context: PathBuf,
        port: u16,
        registry: String,
        system: String,
        update: bool,
        variable: Vec<String>,
    ) -> Result<Self> {
        let lock = load_lockfile(&artifact_context.join("Vorpal.lock"))?;

        Ok(Self {
            agent,
            artifact,
            artifact_context,
            lock,
            update,
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

    fn hydrate_sources_from_lock(&self, artifact: &mut Artifact) {
        let Some(lock) = &self.lock else {
            return;
        };

        for src in artifact.sources.iter_mut() {
            if src.digest.is_some() {
                continue;
            }

            // Only hydrate remote sources from lockfile; local paths should
            // be re-hashed so lockfile can reflect source lifecycle changes.
            let is_http = src.path.starts_with("http://") || src.path.starts_with("https://");
            if !is_http {
                continue;
            }

            for s in &lock.sources {
                if s.name != src.name {
                    continue;
                }

                if let Some(art) = &s.artifact {
                    if art != &artifact.name {
                        continue;
                    }
                } else {
                    continue;
                }

                let path_match = false; // never match local paths; we skip above
                let url_match = s.url.as_deref() == Some(src.path.as_str());

                if (path_match || url_match) && !s.digest.is_empty() {
                    src.digest = Some(s.digest.clone());
                    break;
                }
            }
        }
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

        // Prepare a local copy hydrated from lockfile with known remote digests
        // Hydration helps resume interrupted runs and avoid re-downloading prepared sources.
        let mut artifact_to_prepare = artifact.clone();

        self.hydrate_sources_from_lock(&mut artifact_to_prepare);

        let artifact_json =
            serde_json::to_vec(&artifact_to_prepare).expect("failed to serialize artifact to JSON");

        let artifact_digest = digest(artifact_json);

        if self.store.artifact.contains_key(&artifact_digest) {
            return Ok(artifact_digest);
        }

        // TODO: make this run in parallel

        let mut client = AgentServiceClient::connect(self.agent.clone())
            .await
            .expect("failed to connect to agent service");

        let request = PrepareArtifactRequest {
            artifact: Some(artifact_to_prepare.clone()),
            artifact_context: self.artifact_context.display().to_string(),
            update_mode: self.update,
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

        let mut client = ArtifactServiceClient::connect(self.registry.clone())
            .await
            .expect("failed to connect to artifact service");

        // Treat 64-hex strings as direct digests; otherwise resolve via alias
        if is_hex_digest(alias) {
            let request = ArtifactRequest {
                digest: alias.to_string(),
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

    #[allow(dead_code)]
    fn persist_remote_sources_for_artifact(&self, artifact: &Artifact) -> Result<()> {
        let lock_path = self.artifact_context.join("Vorpal.lock");

        let mut lock = match crate::lock::load(&lock_path)? {
            Some(l) => l,
            None => Lockfile {
                lockfile: 1,
                sources: vec![],
                artifacts: vec![],
            },
        };

        for src in &artifact.sources {
            let is_http = src.path.starts_with("http://") || src.path.starts_with("https://");
            if !is_http {
                continue;
            }

            let digest = match &src.digest {
                Some(d) if !d.is_empty() => d.clone(),
                _ => continue,
            };

            let key_name = src.name.clone();
            let key_url = src.path.clone();
            let artifact_name = artifact.name.clone();

            if let Some(existing) = lock.sources.iter_mut().find(|s| {
                s.kind == "http"
                    && s.name == key_name
                    && s.artifact.as_deref() == Some(artifact_name.as_str())
                    && s.url.as_deref() == Some(key_url.as_str())
            }) {
                existing.digest = digest.clone();
                existing.includes = src.includes.clone();
                existing.excludes = src.excludes.clone();
            } else {
                lock.sources.push(crate::lock::LockSource {
                    name: key_name,
                    kind: "http".to_string(),
                    path: None,
                    url: Some(key_url),
                    includes: src.includes.clone(),
                    excludes: src.excludes.clone(),
                    digest,
                    rev: None,
                    artifact: Some(artifact_name),
                });
            }
        }

        // Policy: persist only remote sources
        lock.sources.retain(|s| s.kind != "local");
        lock.sources
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

        save_lockfile(&lock_path, &lock)?;
        Ok(())
    }

    #[allow(dead_code)]
    fn persist_artifact_if_remote_only(
        &self,
        artifact: &Artifact,
        artifact_digest: &str,
    ) -> Result<()> {
        // Only persist artifacts composed solely of remote sources
        let has_local_source = artifact.sources.iter().any(|src| {
            let is_http = src.path.starts_with("http://") || src.path.starts_with("https://");
            !is_http
        });

        if has_local_source {
            return Ok(());
        }

        let lock_path = self.artifact_context.join("Vorpal.lock");

        let mut lock = match crate::lock::load(&lock_path)? {
            Some(l) => l,
            None => Lockfile {
                lockfile: 1,
                sources: vec![],
                artifacts: vec![],
            },
        };

        let deps = artifact
            .steps
            .iter()
            .flat_map(|s| s.artifacts.clone())
            .collect::<Vec<String>>();

        let systems = artifact
            .systems
            .iter()
            .map(|s| {
                ArtifactSystem::try_from(*s)
                    .map(|v| v.as_str_name().to_string())
                    .unwrap_or_else(|_| s.to_string())
            })
            .collect::<Vec<String>>();

        if let Some(existing) = lock.artifacts.iter_mut().find(|a| a.name == artifact.name) {
            existing.digest = artifact_digest.to_string();
            existing.aliases = artifact.aliases.clone();
            existing.systems = systems;
            existing.deps = deps;
        } else {
            lock.artifacts.push(crate::lock::LockArtifact {
                name: artifact.name.clone(),
                digest: artifact_digest.to_string(),
                aliases: artifact.aliases.clone(),
                systems,
                deps,
            });
        }

        lock.artifacts
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

        save_lockfile(&lock_path, &lock)?;
        Ok(())
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
