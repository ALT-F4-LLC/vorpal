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

            // ADVISORY ONLY, not the exclusive create `LocalBackend` gets.
            // The head and the put below are separate requests with no state
            // carried between them, so two publishers of an absent alias can
            // both pass this check and both put: last writer wins and both
            // callers are told `Ok`. What it does buy is a *sequential*
            // republish under a different digest being refused rather than
            // silently overwriting, which is the case it was added for.
            //
            // `LocalBackend` no longer has the `alias_path.exists()` this
            // once claimed parity with: `publish_alias` in `local.rs`
            // publishes by `hard_link`, so its refusal comes from the create
            // itself and holds under concurrency.
            //
            // The S3 equivalent is a conditional `put_object` carrying
            // `If-None-Match: *`, which is atomic at the storage service.
            // It is not used here because it is only safe once the endpoints
            // this registry is deployed against are known to honour it, and
            // they are not: the client is built from ambient configuration
            // (`aws_config::defaults` in `registry.rs`), no supported
            // endpoint set is declared anywhere in this repository, and
            // `--registry-backend-s3-force-path-style` exists precisely so
            // non-AWS S3-compatible servers can be targeted. AWS S3 honours
            // the header; MinIO's wildcard support is contested across
            // versions (minio/minio#20346, closed "working as intended"
            // stating `*` is unsupported, against a current source that
            // special-cases it); Ceph/RGW, R2 and the GCS interoperability
            // layer are unchecked. An endpoint that accepted the header and
            // ignored it would lose even the sequential refusal above, so
            // this check stays until that set is declared.
            //
            // `head_object`'s error case must be inspected, not treated as
            // "absent": S3's own throttling, a transient network failure, or
            // an `AccessDenied` on this one key all surface as the same
            // `Err` as a genuine 404, and a caller who can force one of
            // those (or is merely unlucky) previously overwrote an alias
            // this check exists to protect (VPL-383 CLUSTER-18). Only a
            // modeled "not found" is treated as absence; every other error
            // propagates as a failure instead of silently proceeding.
            //
            // An alias already holding this exact digest is the state the
            // caller asked for, so it is left alone rather than refused: a
            // publication that failed part way through is completed by
            // re-sending the same request. This matches
            // `publish_alias` in `local.rs`. The existing digest is read here
            // rather than through `get_artifact_alias` so a get that fails
            // after a successful head propagates as a failure instead of the
            // `not_found` that method reports for any get error.
            match client
                .head_object()
                .bucket(bucket)
                .key(&alias_key)
                .send()
                .await
            {
                Ok(_) => {
                    let mut published_stream = client
                        .get_object()
                        .bucket(bucket)
                        .key(&alias_key)
                        .send()
                        .await
                        .map_err(|err| {
                            Status::internal(format!("failed to read existing alias: {err}"))
                        })?
                        .body;

                    let mut published_digest = String::new();

                    while let Some(chunk) = published_stream.next().await {
                        let published_chunk = chunk.map_err(|err| {
                            Status::internal(format!("failed to read existing alias: {err}"))
                        })?;

                        published_digest.push_str(&String::from_utf8_lossy(&published_chunk));
                    }

                    if published_digest == artifact_digest {
                        continue;
                    }

                    return Err(Status::already_exists(format!(
                        "alias '{alias}' already exists"
                    )));
                }
                Err(err) => {
                    let is_not_found = err.as_service_error().is_some_and(
                        aws_sdk_s3::operation::head_object::HeadObjectError::is_not_found,
                    );

                    if !is_not_found {
                        return Err(Status::internal(format!(
                            "failed to check alias existence: {err}"
                        )));
                    }
                }
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
