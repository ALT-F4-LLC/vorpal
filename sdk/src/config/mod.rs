use crate::service;
use anyhow::Result;
use clap::{Parser, Subcommand};
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
