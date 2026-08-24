use crate::command::{
    start::registry::{
        s3::{get_artifact_alias_key, get_artifact_config_key},
        ArtifactBackend, S3Backend,
    },
    store::paths::split_alias_name_tag,
};
use sha256::digest;
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::{Artifact, ArtifactSystem};

#[async_trait]
impl ArtifactBackend for S3Backend {
    async fn get_artifact(
        &self,
        artifact_digest: &str,
        artifact_namespace: &str,
    ) -> Result<Artifact, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let artifact_key = get_artifact_config_key(artifact_digest, artifact_namespace);

        client
            .head_object()
            .bucket(bucket)
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
        name: &str,
        namespace: &str,
        system: ArtifactSystem,
        version: &str,
    ) -> Result<String, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let alias_key = get_artifact_alias_key(name, namespace, system, version);

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

        let artifact_system = artifact.target();

        let aliases = [artifact.aliases, artifact_aliases]
            .concat()
            .into_iter()
            .collect::<Vec<String>>();

        for alias in aliases {
            let (alias_name, alias_tag) = split_alias_name_tag(&alias);

            if alias_name.is_empty() {
                continue;
            }

            // `alias_name` and `alias_tag` are already validated by
            // `parse_alias_name` / `parse_store_path_component` in the
            // `ArtifactService::store_artifact` handler (`registry.rs`),
            // which runs before this backend is ever called (VPL-383). The
            // split itself is shared with that handler and the local
            // backend (`split_alias_name_tag`), so the pair validated there
            // is the same pair joined into a key here.

            let alias_key =
                get_artifact_alias_key(alias_name, &artifact_namespace, artifact_system, alias_tag);

            // Parity with `LocalBackend`, which refuses to overwrite an
            // existing alias (`local.rs`, `alias_path.exists()`): without
            // this check the S3 backend silently overwrote an alias that
            // pointed at a different digest, which `LocalBackend` treats as
            // a conflict rather than a no-op or a silent republish.
            let alias_head = client
                .head_object()
                .bucket(bucket)
                .key(&alias_key)
                .send()
                .await;

            if alias_head.is_ok() {
                return Err(Status::already_exists(format!(
                    "alias '{}' already exists",
                    alias
                )));
            }

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
