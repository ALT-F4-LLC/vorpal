use crate::command::{
    start::registry::{ArtifactBackend, LocalBackend},
    store::paths::{
        discard_staging, get_artifact_alias_path, get_artifact_config_path, set_timestamps,
        split_alias_name_tag, staging_path_for,
    },
};
use sha256::digest;
use std::{io::ErrorKind, path::Path};
use tokio::fs::{create_dir_all, hard_link, read, write};
use tonic::{async_trait, Status};
use vorpal_sdk::api::artifact::{Artifact, ArtifactSystem};

/// Creates the alias file from a fully-written staged sibling, so a concurrent
/// `get_artifact_alias` observes either no alias or the whole digest, and
/// reports `already_exists` from the create itself rather than from a prior
/// stat.
///
/// The publish is a hard link, not the `publish_atomically` rename the rest of
/// the store uses. A rename onto an existing *file* succeeds and replaces it,
/// so publishing an alias that way would silently overwrite the mapping a name
/// already has — the immutability this backend promises. `hard_link` fails
/// `AlreadyExists` when the name is taken, including when the name is a
/// dangling symlink an untrusted build step left behind, and never writes
/// through it.
///
/// A taken name that already holds this exact digest is the state the caller
/// asked for, so it is accepted: a publication that failed part way through is
/// completed by re-sending the same request. The refusal stays for a
/// name mapped to a different digest, and for an entry whose contents cannot be
/// read back as this digest — a symlink to an unreadable or foreign target
/// included.
async fn publish_alias(
    alias_path: &Path,
    alias: &str,
    artifact_digest: &str,
) -> Result<(), Status> {
    let staging_path = staging_path_for(alias_path);

    if let Err(err) = write(&staging_path, artifact_digest).await {
        discard_staging(&staging_path).await;

        return Err(Status::internal(format!("failed to write alias: {err}")));
    }

    if let Err(err) = set_timestamps(&staging_path).await {
        discard_staging(&staging_path).await;

        return Err(Status::internal(format!("failed to sanitize alias: {err}")));
    }

    let link = hard_link(&staging_path, alias_path).await;

    discard_staging(&staging_path).await;

    match link {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {
            let published_digest = read(alias_path).await.ok();

            if published_digest.as_deref() == Some(artifact_digest.as_bytes()) {
                return Ok(());
            }

            Err(Status::already_exists(format!(
                "alias '{}' already exists",
                alias
            )))
        }
        Err(err) => Err(Status::internal(format!("failed to publish alias: {err}"))),
    }
}

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
            let (alias_name, alias_tag) = split_alias_name_tag(&alias);

            if alias_name.is_empty() {
                continue;
            }

            // `alias_name` and `alias_tag` are already validated by
            // `parse_alias_name` / `parse_store_path_component` in the
            // `ArtifactService::store_artifact` handler (`registry.rs`),
            // which runs before this backend is ever called (VPL-383). The
            // split itself is shared with that handler and the S3 backend
            // (`split_alias_name_tag`), so the pair validated there is the
            // same pair joined into a path here.

            let alias_path = get_artifact_alias_path(
                alias_name,
                &artifact_namespace,
                artifact_system,
                alias_tag,
            )
            .map_err(|err| Status::internal(format!("failed to get artifact alias path: {err}")))?;

            if let Some(parent) = alias_path.parent() {
                if !parent.exists() {
                    create_dir_all(parent).await.map_err(|err| {
                        Status::internal(format!("failed to create alias dir: {err}"))
                    })?;
                }
            }

            publish_alias(&alias_path, &alias, &artifact_digest).await?;
        }

        Ok(artifact_digest)
    }

    fn box_clone(&self) -> Box<dyn ArtifactBackend> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use tempfile::TempDir;
    use tonic::Code;

    // The store root is a fixed absolute path, so `store_artifact` itself
    // cannot be exercised against a temporary directory. `publish_alias` is
    // the seam that carries every property under test: it owns the create,
    // the conflict decision and the staging cleanup.
    fn dir_entry_names(dir: &Path) -> BTreeSet<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    // Two publishers race for one alias name with different digests. Exactly
    // one may win, the other must be told the name is taken rather than
    // replacing the mapping, and the digest left on disk must be the winner's.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn one_of_two_concurrent_publishers_of_one_alias_name_is_refused() {
        let root = TempDir::new().unwrap();
        let alias_path = root.path().join("latest");

        let mut publishes = Vec::new();

        for source in ["first-artifact", "second-artifact"] {
            let path = alias_path.clone();
            let artifact_digest = digest(source);

            publishes.push(tokio::spawn(async move {
                publish_alias(&path, "rust:latest", &artifact_digest)
                    .await
                    .map(|()| artifact_digest)
            }));
        }

        let mut published = Vec::new();
        let mut refused = Vec::new();

        for publish in publishes {
            match publish.await.unwrap() {
                Ok(artifact_digest) => published.push(artifact_digest),
                Err(status) => refused.push(status),
            }
        }

        assert_eq!(published.len(), 1, "expected exactly one publisher to win");
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].code(), Code::AlreadyExists);
        assert_eq!(
            std::fs::read_to_string(&alias_path).unwrap(),
            published[0],
            "the alias must hold the winner's digest"
        );
        assert_eq!(
            dir_entry_names(root.path()),
            BTreeSet::from(["latest".to_string()]),
            "the loser must leave no staging file behind"
        );
    }

    // A reader polling the alias path while publishers work must never see a
    // partially written digest: the alias name is created by an atomic link to
    // a file that is already complete.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_concurrent_reader_never_observes_a_partial_digest() {
        const ROUNDS: usize = 250;

        let root = TempDir::new().unwrap();
        let alias_dir = root.path().to_path_buf();
        let artifact_digest = digest("concurrently-read-artifact");

        let reader_dir = alias_dir.clone();

        let reader = tokio::task::spawn_blocking(move || {
            let mut observed = Vec::new();

            for round in 0..ROUNDS {
                let alias_path = reader_dir.join(format!("tag-{round}"));

                for _ in 0..10_000 {
                    if let Ok(value) = std::fs::read_to_string(&alias_path) {
                        observed.push(value);
                        break;
                    }
                }
            }

            observed
        });

        for round in 0..ROUNDS {
            let alias_path = alias_dir.join(format!("tag-{round}"));

            publish_alias(&alias_path, "rust:latest", &artifact_digest)
                .await
                .unwrap();
        }

        let observed = reader.await.unwrap();

        assert!(
            !observed.is_empty(),
            "the reader never caught a published alias"
        );

        for value in observed {
            assert_eq!(value, artifact_digest);
        }
    }

    // A publication that failed part way through is completed by re-sending the
    // same request. Re-publishing an alias that already holds this exact digest
    // is the state the caller asked for, so it must succeed rather than refuse.
    #[tokio::test]
    async fn republishing_an_alias_with_the_same_digest_succeeds() {
        let root = TempDir::new().unwrap();
        let alias_path = root.path().join("latest");
        let artifact_digest = digest("artifact");

        publish_alias(&alias_path, "rust:latest", &artifact_digest)
            .await
            .unwrap();

        publish_alias(&alias_path, "rust:latest", &artifact_digest)
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(&alias_path).unwrap(),
            artifact_digest
        );
        assert_eq!(
            dir_entry_names(root.path()),
            BTreeSet::from(["latest".to_string()]),
            "the republish must leave no staging file behind"
        );
    }

    // An alias name already mapped to another digest is a real conflict: the
    // immutability the backend promises is what makes the same-digest case safe
    // to accept.
    #[tokio::test]
    async fn publishing_an_alias_holding_a_different_digest_is_refused() {
        let root = TempDir::new().unwrap();
        let alias_path = root.path().join("latest");
        let published_digest = digest("first-artifact");

        publish_alias(&alias_path, "rust:latest", &published_digest)
            .await
            .unwrap();

        let status = publish_alias(&alias_path, "rust:latest", &digest("second-artifact"))
            .await
            .unwrap_err();

        assert_eq!(status.code(), Code::AlreadyExists);
        assert_eq!(
            std::fs::read_to_string(&alias_path).unwrap(),
            published_digest,
            "the refused publish must not replace the mapping"
        );
    }

    // A build step runs as the daemon's own uid and can drop a symlink at an
    // alias path before the alias is published. Publishing must fail on the
    // entry that is already there rather than writing through it into whatever
    // it points at.
    #[tokio::test]
    async fn a_dangling_symlink_at_the_alias_path_is_not_written_through() {
        let root = TempDir::new().unwrap();
        let alias_path = root.path().join("latest");
        let symlink_target = root.path().join("credentials.json");

        std::os::unix::fs::symlink(&symlink_target, &alias_path).unwrap();

        let status = publish_alias(&alias_path, "rust:latest", &digest("artifact"))
            .await
            .unwrap_err();

        assert_eq!(status.code(), Code::AlreadyExists);
        assert!(
            !symlink_target.exists(),
            "the symlink target must not have been created"
        );
        assert_eq!(
            dir_entry_names(root.path()),
            BTreeSet::from(["latest".to_string()]),
            "a refused publish must leave no staging file behind"
        );
    }
}
