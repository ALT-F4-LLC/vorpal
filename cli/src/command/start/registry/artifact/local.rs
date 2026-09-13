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
    async fn get_artifact(&self, digest: &str, namespace: &str) -> Result<Artifact, Status> {
        let artifact_config_path = get_artifact_config_path(digest, namespace);

        if !artifact_config_path.exists() {
            return Err(Status::not_found("config not found"));
        }

        let artifact_config_data = read(&artifact_config_path)
            .await
            .map_err(|err| Status::internal(format!("failed to read config: {err}")))?;

        let artifact: Artifact = serde_json::from_slice(&artifact_config_data)
            .map_err(|err| Status::internal(format!("failed to parse config: {err}")))?;

        Ok(artifact)
    }

    async fn get_artifact_alias(
        &self,
        name: &str,
        namespace: &str,
        system: ArtifactSystem,
        tag: &str,
    ) -> Result<String, Status> {
        let artifact_alias_path = get_artifact_alias_path(name, namespace, system, tag)
            .map_err(|err| Status::internal(format!("failed to get artifact alias path: {err}")))?;

        if !artifact_alias_path.exists() {
            return Err(Status::not_found("alias not found"));
        }

        let artifact_digest = read(&artifact_alias_path)
            .await
            .map_err(|err| Status::internal(format!("failed to read alias: {err}")))?;

        let artifact_digest = String::from_utf8(artifact_digest)
            .map_err(|err| Status::internal(format!("failed to parse alias: {err}")))?;

        Ok(artifact_digest)
    }

    async fn store_artifact(
        &self,
        artifact: Artifact,
        artifact_aliases: Vec<String>,
        artifact_namespace: String,
    ) -> Result<String, Status> {
        let artifact_json = serde_json::to_vec(&artifact)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {err}")))?;
        let artifact_digest = digest(&artifact_json);
        let artifact_config_path = get_artifact_config_path(&artifact_digest, &artifact_namespace);

        if !artifact_config_path.exists() {
            if let Some(parent) = artifact_config_path.parent() {
                if !parent.exists() {
                    create_dir_all(parent).await.map_err(|err| {
                        Status::internal(format!("failed to create config dir: {err}"))
                    })?;
                }
            }

            write(&artifact_config_path, artifact_json)
                .await
                .map_err(|err| Status::internal(format!("failed to write store config: {err}")))?;

            set_timestamps(&artifact_config_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize path: {err}")))?;
        }

        let artifact_system = artifact.target();

        let aliases = [artifact.aliases, artifact_aliases]
            .concat()
            .into_iter()
            .collect::<Vec<String>>();

        for alias in aliases {
            let alias_name = alias.split(':').next().unwrap_or(&alias);

            if alias_name.is_empty() {
                continue;
            }

            // TODO: validate alias name and tag

            if alias_name.len() > 255 {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' is too long (max 255 characters)"
                )));
            }

            if alias_name.contains('/') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' cannot contain '/'"
                )));
            }

            if alias_name.contains('\\') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' cannot contain '\\'"
                )));
            }

            if alias_name.contains('\0') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' cannot contain null bytes"
                )));
            }

            if alias_name.starts_with('.') || alias_name.ends_with('.') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' cannot start or end with '.'"
                )));
            }

            if alias_name.starts_with('-') || alias_name.ends_with('-') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' cannot start or end with '-'"
                )));
            }

            if alias_name.chars().any(char::is_whitespace) {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' cannot contain whitespace"
                )));
            }

            if alias_name
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.')
            {
                return Err(Status::invalid_argument(format!(
                    "alias name '{alias_name}' can only contain alphanumeric characters, '_', '-', and '.'"
                )));
            }

            let alias_tag = alias.split(':').nth(1).unwrap_or("latest").to_string();

            let alias_path = get_artifact_alias_path(
                alias_name,
                &artifact_namespace,
                artifact_system,
                &alias_tag,
            )
            .map_err(|err| Status::internal(format!("failed to get artifact alias path: {err}")))?;

            if let Some(parent) = alias_path.parent() {
                if !parent.exists() {
                    create_dir_all(parent).await.map_err(|err| {
                        Status::internal(format!("failed to create alias dir: {err}"))
                    })?;
                }
            }

            if alias_path.exists() {
                return Err(Status::already_exists(format!(
                    "alias '{alias}' already exists"
                )));
            }

            write(&alias_path, &artifact_digest)
                .await
                .map_err(|err| Status::internal(format!("failed to write alias: {err}")))?;

            set_timestamps(&alias_path)
                .await
                .map_err(|err| Status::internal(format!("failed to sanitize alias: {err}")))?;
        }

        Ok(artifact_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
