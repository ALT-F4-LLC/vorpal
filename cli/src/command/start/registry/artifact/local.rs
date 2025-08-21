use crate::command::{
    start::registry::{ArtifactBackend, LocalBackend},
    store::paths::{get_artifact_alias_path, get_artifact_config_path, set_timestamps},
};
use sha256::digest;
use tokio::fs::{create_dir_all, read, write};
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::{Artifact, ArtifactSystem};

#[async_trait]
impl ArtifactBackend for LocalBackend {
    async fn get_artifact(&self, digest: String) -> Result<Artifact, Status> {
        let config_path = get_artifact_config_path(&digest);

        if !config_path.exists() {
            return Err(Status::not_found("config not found"));
        }

        let config_data = read(&config_path)
            .await
            .map_err(|err| Status::internal(format!("failed to read config: {err}")))?;

        let artifact: Artifact = serde_json::from_slice(&config_data)
            .map_err(|err| Status::internal(format!("failed to parse config: {err}")))?;

        Ok(artifact)
    }

    async fn get_artifact_alias(
        &self,
        alias: String,
        alias_system: ArtifactSystem,
    ) -> Result<String, Status> {
        let alias_path = get_artifact_alias_path(&alias, alias_system)
            .map_err(|err| Status::internal(format!("failed to get artifact alias path: {err}")))?;

        if !alias_path.exists() {
            return Err(Status::not_found("alias not found"));
        }

        let digest = read(&alias_path)
            .await
            .map_err(|err| Status::internal(format!("failed to read alias: {err}")))?;

        let digest = String::from_utf8(digest.to_vec())
            .map_err(|err| Status::internal(format!("failed to parse alias: {err}")))?;

        Ok(digest)
    }

    async fn store_artifact(
        &self,
        artifact: Artifact,
        artifact_aliases: Vec<String>,
    ) -> Result<String, Status> {
        let config_json = serde_json::to_vec(&artifact)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {err}")))?;
        let config_digest = digest(&config_json);
        let config_path = get_artifact_config_path(&config_digest);

        if !config_path.exists() {
            write(&config_path, config_json)
                .await
                .map_err(|err| Status::internal(format!("failed to write store config: {err}")))?;

            set_timestamps(&config_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize path: {err}")))?;
        }

        let aliases = [artifact.clone().aliases, artifact_aliases]
            .concat()
            .into_iter()
            .collect::<Vec<String>>();

        let alias_system = artifact.target();

        for alias in aliases {
            let alias_path = get_artifact_alias_path(&alias, alias_system).map_err(|err| {
                Status::internal(format!("failed to get artifact alias path: {err}"))
            })?;

            if let Some(parent) = alias_path.parent() {
                if !parent.exists() {
                    create_dir_all(parent).await.map_err(|err| {
                        Status::internal(format!("failed to create alias dir: {err}"))
                    })?;
                }
            }

            write(&alias_path, &config_digest)
                .await
                .map_err(|err| Status::internal(format!("failed to write alias: {err}")))?;

            set_timestamps(&alias_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize alias: {err}")))?;
        }

        Ok(config_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
