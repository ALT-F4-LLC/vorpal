use crate::command::start::registry::{
    s3::{get_artifact_alias_key, get_artifact_config_key},
    ArtifactBackend, S3Backend,
};
use sha256::digest;
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::{Artifact, ArtifactSystem};

#[async_trait]
impl ArtifactBackend for S3Backend {
    async fn get_artifact(&self, artifact_digest: String) -> Result<Artifact, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_key = get_artifact_config_key(&artifact_digest);

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
            .map_err(|err| Status::internal(format!("failed to parse artifact: {:?}", err)))?;

        Ok(artifact)
    }

    async fn get_artifact_alias(
        &self,
        artifact_alias: String,
        artifact_system: ArtifactSystem,
    ) -> Result<String, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let alias_key = get_artifact_alias_key(&artifact_alias, artifact_system.as_str_name())
            .map_err(|err| {
                Status::internal(format!("failed to get artifact alias key: {:?}", err))
            })?;

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
    ) -> Result<String, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let config_json = serde_json::to_vec(&artifact)
            .map_err(|err| Status::internal(format!("failed to serialize artifact: {:?}", err)))?;
        let config_digest = digest(&config_json);
        let config_key = get_artifact_config_key(&config_digest);

        let config_head = client
            .head_object()
            .bucket(bucket)
            .key(&config_key)
            .send()
            .await;

        if config_head.is_err() {
            client
                .put_object()
                .bucket(bucket)
                .key(config_key)
                .body(config_json.into())
                .send()
                .await
                .map_err(|err| Status::internal(format!("failed to write config: {:?}", err)))?;
        }

        let aliases = [artifact.clone().aliases, artifact_aliases]
            .concat()
            .into_iter()
            .collect::<Vec<String>>();

        let alias_system = artifact.target().as_str_name();

        for alias in aliases {
            let alias_key = get_artifact_alias_key(&alias, alias_system).map_err(|err| {
                Status::internal(format!("failed to get artifact alias key: {:?}", err))
            })?;

            let alias_data = config_digest.as_bytes().to_vec();

            client
                .put_object()
                .bucket(bucket)
                .key(alias_key)
                .body(alias_data.into())
                .send()
                .await
                .map_err(|err| Status::internal(format!("failed to write alias: {:?}", err)))?;
        }

        Ok(config_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}
