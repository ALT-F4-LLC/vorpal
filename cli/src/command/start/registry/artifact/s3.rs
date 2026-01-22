use crate::command::start::registry::{
    artifact::function::{resolve_function, ArtifactFunctionDefinition},
    s3::{get_artifact_alias_key, get_artifact_config_key, get_artifact_function_key},
    ArtifactBackend, S3Backend,
};
use sha256::digest;
use std::collections::HashMap;
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::{Artifact, ArtifactFunction, ArtifactSystem};

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

    async fn get_artifact_functions(
        &self,
        namespace: String,
        name_prefix: String,
    ) -> Result<Vec<ArtifactFunction>, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let prefix = if namespace.is_empty() {
            "artifact/function/".to_string()
        } else {
            format!("artifact/function/{namespace}/")
        };

        let name_prefix = if name_prefix.is_empty() {
            None
        } else {
            Some(name_prefix)
        };

        let mut functions = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = client.list_objects_v2().bucket(bucket).prefix(&prefix);

            if let Some(token) = continuation_token.as_deref() {
                request = request.continuation_token(token);
            }

            let response = request
                .send()
                .await
                .map_err(|err| Status::internal(err.to_string()))?;

            for object in response.contents() {
                let Some(key) = object.key() else {
                    continue;
                };

                if !key.ends_with(".json") {
                    continue;
                }

                let Some(function_name) = parse_function_key_name(key) else {
                    continue;
                };

                if let Some(prefix) = name_prefix.as_deref() {
                    if !function_name.starts_with(prefix) {
                        continue;
                    }
                }

                let definition = load_function_definition_from_key(client, bucket, key).await?;
                functions.push(definition.meta);
            }

            continuation_token = response
                .next_continuation_token()
                .map(|token| token.to_string());

            if continuation_token.is_none() {
                break;
            }
        }

        Ok(functions)
    }

    async fn get_artifact_function(
        &self,
        name: String,
        namespace: String,
        tag: String,
        system: ArtifactSystem,
        params: HashMap<String, String>,
    ) -> Result<Artifact, Status> {
        let client = &self.client;
        let bucket = &self.bucket;

        let tag = if tag.is_empty() { "latest" } else { &tag };
        let key = get_artifact_function_key(&name, &namespace, tag);

        let definition = load_function_definition_from_key(client, bucket, &key).await?;

        resolve_function(definition, params, system)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}

async fn load_function_definition_from_key(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
) -> Result<ArtifactFunctionDefinition, Status> {
    let mut object_stream = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(map_get_object_error)?
        .body;

    let mut definition_json = String::new();

    while let Some(chunk) = object_stream.next().await {
        let chunk = chunk.map_err(|err| Status::internal(err.to_string()))?;
        definition_json.push_str(&String::from_utf8_lossy(&chunk));
    }

    let definition: ArtifactFunctionDefinition = serde_json::from_str(&definition_json)
        .map_err(|err| Status::internal(format!("failed to parse function definition: {err}")))?;

    Ok(definition)
}

fn parse_function_key_name(key: &str) -> Option<&str> {
    let parts: Vec<&str> = key.split('/').collect();

    if parts.len() != 5 {
        return None;
    }

    if parts[0] != "artifact" || parts[1] != "function" {
        return None;
    }

    if !parts[4].ends_with(".json") {
        return None;
    }

    Some(parts[3])
}

fn map_get_object_error(
    err: aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::get_object::GetObjectError>,
) -> Status {
    match err {
        aws_sdk_s3::error::SdkError::ServiceError(service_err) => {
            if service_err.err().is_no_such_key() {
                Status::not_found(service_err.err().to_string())
            } else {
                Status::internal(service_err.err().to_string())
            }
        }
        _ => Status::internal(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_function_key_name;

    #[test]
    fn test_parse_function_key_name_valid() {
        let key = "artifact/function/default/hello/latest.json";
        assert_eq!(parse_function_key_name(key), Some("hello"));
    }

    #[test]
    fn test_parse_function_key_name_invalid() {
        assert_eq!(parse_function_key_name("artifact/functions/ns/name/tag.json"), None);
        assert_eq!(parse_function_key_name("artifact/function/ns/name/tag"), None);
        assert_eq!(parse_function_key_name("artifact/function/ns/name/tag.json/extra"), None);
        assert_eq!(parse_function_key_name("artifact/function/ns/name.json"), None);
    }
}
