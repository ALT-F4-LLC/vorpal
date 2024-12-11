use crate::config::service::ConfigServer;
use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use std::path::PathBuf;
use tokio::fs::{create_dir_all, remove_dir_all};
use tonic::transport::Server;
use tracing::Level;
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{Artifact, ArtifactId, ArtifactSystem},
        config::v0::{config_service_server::ConfigServiceServer, Config},
    },
};
use vorpal_store::{hashes::hash_files, paths::copy_files, temps::create_sandbox_dir};

pub mod artifact;
pub mod service;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn get_default_system() -> String {
    format!("{}-{}", ARCH, OS)
}

#[derive(Subcommand)]
enum Command {
    Start {
        #[clap(default_value_t = Level::INFO, global = true, long)]
        level: Level,

        #[clap(long, short)]
        port: u16,

        #[arg(default_value_t = get_default_system(), long, short)]
        target: String,
    },
}

pub async fn get_context() -> Result<ConfigContext> {
    let args = Cli::parse();

    match args.command {
        Command::Start { port, target, .. } => {
            let target = get_artifact_system::<ArtifactSystem>(&target);

            if target == ArtifactSystem::UnknownSystem {
                return Err(anyhow::anyhow!("Invalid target system"));
            }

            Ok(ConfigContext::new(port, target))
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConfigContext {
    artifact_id: HashMap<String, Artifact>,
    pub port: u16,
    source_hash: HashMap<String, String>,
    system: ArtifactSystem,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub system: ArtifactSystem,
}

fn get_source_key_digest(path: PathBuf, files: Vec<PathBuf>) -> Result<String> {
    let mut relative_paths = vec![];

    for file in files {
        let relative_path = file.strip_prefix(&path).map_err(|e| anyhow::anyhow!(e))?;

        relative_paths.push(relative_path.display().to_string());
    }

    let relative_paths = relative_paths.join("\n");

    let source_hash = digest(relative_paths.as_bytes());

    Ok(source_hash)
}

impl ConfigContext {
    pub fn new(port: u16, system: ArtifactSystem) -> Self {
        Self {
            artifact_id: HashMap::new(),
            port,
            source_hash: HashMap::new(),
            system,
        }
    }

    pub fn add_artifact(&mut self, artifact: Artifact) -> Result<ArtifactId> {
        let artifact_json = serde_json::to_string(&artifact).map_err(|e| anyhow::anyhow!(e))?;
        let artifact_metadata = ArtifactMetadata {
            system: self.system,
        };
        let artifact_metadata_json =
            serde_json::to_string(&artifact_metadata).map_err(|e| anyhow::anyhow!(e))?;
        let artifact_manifest = format!("{}:{}", artifact_json, artifact_metadata_json);
        let artifact_manifest_hash = digest(artifact_manifest.as_bytes());
        let artifact_key = format!("{}-{}", artifact.name, artifact_manifest_hash);

        if !self.artifact_id.contains_key(&artifact_key) {
            self.artifact_id
                .insert(artifact_key.clone(), artifact.clone());
        }

        let artifact_id = ArtifactId {
            hash: artifact_manifest_hash,
            name: artifact.name,
        };

        Ok(artifact_id)
    }

    pub fn get_artifact(&self, hash: &str, name: &str) -> Option<&Artifact> {
        let artifact_key = format!("{}-{}", name, hash);
        self.artifact_id.get(&artifact_key)
    }

    pub async fn add_source_hash(
        &mut self,
        files: Vec<PathBuf>,
        name: String,
        path: PathBuf,
    ) -> Result<String> {
        let source_key_digest = get_source_key_digest(path.clone(), files.clone())?;
        let source_key = format!("{}-{}", name, source_key_digest);

        if !self.source_hash.contains_key(&source_key) {
            let sandbox_path = create_sandbox_dir().await?;

            create_dir_all(&sandbox_path)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            let sandbox_files = copy_files(&path, files.clone(), &sandbox_path).await?;

            let source_hash = hash_files(sandbox_files.clone())?;

            self.source_hash
                .insert(source_key.clone(), source_hash.clone());

            remove_dir_all(&sandbox_path)
                .await
                .map_err(|e| anyhow::anyhow!(e))?;

            return Ok(source_hash);
        }

        let source_hash = self.source_hash.get(&source_key).unwrap();

        Ok(source_hash.clone())
    }

    pub fn get_source_hash(
        &self,
        files: Vec<PathBuf>,
        name: String,
        path: PathBuf,
    ) -> Option<&String> {
        let source_key_digest = get_source_key_digest(path, files).ok()?;
        let source_key = format!("{}-{}", name, source_key_digest);

        self.source_hash.get(&source_key)
    }

    pub fn get_target(&self) -> ArtifactSystem {
        self.system
    }

    pub async fn run(&self, config: Config) -> Result<()> {
        let addr = format!("[::]:{}", self.port)
            .parse()
            .expect("failed to parse address");

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
