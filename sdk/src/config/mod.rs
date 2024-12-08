use crate::service;
use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha256::digest;
use std::collections::HashMap;
use std::env::consts::{ARCH, OS};
use tracing::Level;
use vorpal_schema::{
    get_artifact_system,
    vorpal::{
        artifact::v0::{Artifact, ArtifactId, ArtifactSystem},
        config::v0::Config,
    },
};

pub mod artifact;
pub mod cli;

#[derive(Debug, Default)]
pub struct ContextConfig {
    artifact: HashMap<String, Artifact>,
    system: ArtifactSystem,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub system: ArtifactSystem,
}

impl ContextConfig {
    pub fn new(system: ArtifactSystem) -> Self {
        Self {
            artifact: HashMap::new(),
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

        if !self.artifact.contains_key(&artifact_key) {
            self.artifact.insert(artifact_key.clone(), artifact.clone());
        }

        let artifact_id = ArtifactId {
            hash: artifact_manifest_hash,
            name: artifact.name,
        };

        Ok(artifact_id)
    }

    pub fn get_artifact(&self, hash: &str, name: &str) -> Option<&Artifact> {
        let artifact_key = format!("{}-{}", name, hash);

        self.artifact.get(&artifact_key)
    }

    pub fn get_target(&self) -> ArtifactSystem {
        self.system
    }
}

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

pub type ConfigFunction = fn(context: &mut ContextConfig) -> Result<Config>;

pub async fn execute(config: ConfigFunction) -> Result<()> {
    let args = Cli::parse();

    match args.command {
        Command::Start { port, target, .. } => {
            let target = get_artifact_system::<ArtifactSystem>(&target);

            if target == ArtifactSystem::UnknownSystem {
                return Err(anyhow::anyhow!("Invalid target system"));
            }

            let mut config_context = ContextConfig::new(target);

            let config = config(&mut config_context)?;

            service::listen(config_context, config, port).await?;
        }
    }

    Ok(())
}
