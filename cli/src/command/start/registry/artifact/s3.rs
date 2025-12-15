use crate::command::start::registry::{
    s3::{get_artifact_alias_key, get_artifact_config_key},
    ArtifactBackend, S3Backend,
};
use sha256::digest;
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::{Artifact, ArtifactSystem};

#[async_trait]
impl ArtifactBackend for S3Backend {
    async fn get_artifact(
        &self,
        artifact_digest: String,
        artifact_namespace: String,
    ) -> Result<Artifact, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_key = get_artifact_config_key(&artifact_digest, &artifact_namespace);

        client
            .head_object()
            .bucket(bucket.clone())
            .key(&artifact_key)
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?;

        let mut artifact_stream = client
            .get_object()
            .bucket(bucket)
            .key(&artifact_key)
            .send()
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .body;

        let mut artifact_json = String::new();

        while let Some(chunk) = artifact_stream.next().await {
            let artifact_chunk = chunk.map_err(|err| Status::internal(err.to_string()))?;

            artifact_json.push_str(&String::from_utf8_lossy(&artifact_chunk));
        }

        let artifact: Artifact = serde_json::from_str(&artifact_json)
            .map_err(|err| Status::internal(format!("failed to parse artifact: {err}")))?;

        Ok(artifact)
    }

    async fn get_artifact_alias(
        &self,
        name: String,
        namespace: String,
        system: ArtifactSystem,
        version: String,
    ) -> Result<String, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let alias_key = get_artifact_alias_key(&name, &namespace, system, &version);

        let mut alias_stream = client
            .get_object()
            .bucket(bucket)
            .key(&alias_key)
            .send()
            .await
            .map_err(|err| Status::not_found(err.to_string()))?
            .body;

        let mut alias_digest = String::new();

        while let Some(chunk) = alias_stream.next().await {
            let alias_chunk = chunk.map_err(|err| Status::internal(err.to_string()))?;

            alias_digest.push_str(&String::from_utf8_lossy(&alias_chunk));
        }

        Ok(alias_digest)
    }

    async fn store_artifact(
        &self,
        artifact: Artifact,
        artifact_aliases: Vec<String>,
        artifact_namespace: String,
    ) -> Result<String, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_json = serde_json::to_vec(&artifact)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {err}")))?;
        let artifact_digest = digest(&artifact_json);
        let artifact_config_key = get_artifact_config_key(&artifact_digest, &artifact_namespace);

        let artifact_config_head = client
            .head_object()
            .bucket(bucket)
            .key(&artifact_config_key)
            .send()
            .await;

        if artifact_config_head.is_err() {
            client
                .put_object()
                .bucket(bucket)
                .key(artifact_config_key)
                .body(artifact_json.into())
                .send()
                .await
                .map_err(|err| Status::internal(format!("failed to write config: {err}")))?;
        }

        let aliases = [artifact.clone().aliases, artifact_aliases]
            .concat()
            .into_iter()
            .collect::<Vec<String>>();

        let artifact_system = artifact.target();

        for alias in aliases {
            let alias_name = alias.split(':').next().unwrap_or(&alias);

            if alias_name.is_empty() {
                continue;
            }

            if alias_name.len() > 255 {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' is too long (max 255 characters)",
                    alias_name
                )));
            }

            if alias_name.contains('/') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' cannot contain '/'",
                    alias_name
                )));
            }

            if alias_name.contains('\\') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' cannot contain '\\'",
                    alias_name
                )));
            }

            if alias_name.contains('\0') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' cannot contain null bytes",
                    alias_name
                )));
            }

            if alias_name.starts_with('.') || alias_name.ends_with('.') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' cannot start or end with '.'",
                    alias_name
                )));
            }

            if alias_name.starts_with('-') || alias_name.ends_with('-') {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' cannot start or end with '-'",
                    alias_name
                )));
            }

            if alias_name.chars().any(|c| c.is_whitespace()) {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' cannot contain whitespace",
                    alias_name
                )));
            }

            if alias_name
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.')
            {
                return Err(Status::invalid_argument(format!(
                    "alias name '{}' can only contain alphanumeric characters, '_', '-', and '.'",
                    alias_name
                )));
            }

            let alias_tag = alias.split(':').nth(1).unwrap_or("latest").to_string();

            let alias_key = get_artifact_alias_key(
                alias_name,
                &artifact_namespace,
                artifact_system,
                &alias_tag,
            );

            let alias_data = artifact_digest.as_bytes().to_vec();

            client
                .put_object()
                .bucket(bucket)
                .key(alias_key)
                .body(alias_data.into())
                .send()
                .await
                .map_err(|err| Status::internal(format!("failed to write alias: {err}")))?;
        }

        Ok(artifact_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
