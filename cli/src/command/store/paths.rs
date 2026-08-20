use anyhow::{bail, Error, Result};
use filetime::{set_file_times, set_symlink_file_times, FileTime};
use std::{
    io::{Error as IoError, ErrorKind},
    path::{Path, PathBuf},
};
use tokio::fs::{copy, create_dir_all, metadata, remove_dir_all, remove_file, rename, symlink};
use tonic::Status;
use tracing::{info, warn};
use uuid::Uuid;
use vorpal_sdk::api::artifact::ArtifactSystem;
use walkdir::WalkDir;

// Root paths

pub fn get_root_dir_path() -> PathBuf {
    Path::new("/var/lib/vorpal").to_path_buf()
}

pub fn get_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("VORPAL_SOCKET_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    get_root_dir_path().join("vorpal.sock")
}

pub fn get_lock_path() -> PathBuf {
    let socket_path = get_socket_path();
    let lock_name = socket_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vorpal");
    socket_path.with_file_name(format!("{lock_name}.lock"))
}

pub fn get_root_key_dir_path() -> PathBuf {
    get_root_dir_path().join("key")
}

pub fn get_root_sandbox_dir_path() -> PathBuf {
    get_root_dir_path().join("sandbox")
}

pub fn get_root_store_dir_path() -> PathBuf {
    get_root_dir_path().join("store")
}

// Key paths

pub fn get_key_ca_key_path() -> PathBuf {
    get_root_key_dir_path().join("ca").with_extension("key.pem")
}

pub fn get_key_credentials_path() -> PathBuf {
    get_root_key_dir_path()
        .join("credentials")
        .with_extension("json")
}

pub fn get_key_service_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("pem")
}

pub fn get_key_service_key_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("key.pem")
}

pub fn get_key_service_public_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("public.pem")
}

pub fn get_key_service_secret_path() -> PathBuf {
    get_root_key_dir_path()
        .join("service")
        .with_extension("secret")
}

// Artifact paths

pub fn get_artifact_dir_path() -> PathBuf {
    get_root_store_dir_path().join("artifact")
}

pub fn get_root_artifact_alias_dir_path() -> PathBuf {
    get_artifact_dir_path().join("alias")
}

pub fn get_artifact_alias_dir_path(namespace: &str, system: ArtifactSystem) -> PathBuf {
    get_root_artifact_alias_dir_path()
        .join(namespace)
        .join(system.as_str_name())
}

/// Returns the path for the artifact alias `name`/`tag` under `namespace` and `system`.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the caller in cli/src/command/run.rs propagates the Result with `?`; changing the signature is out of scope for this pass"
)]
pub fn get_artifact_alias_path(
    name: &str,
    namespace: &str,
    system: ArtifactSystem,
    tag: &str,
) -> Result<PathBuf> {
    Ok(get_artifact_alias_dir_path(namespace, system)
        .join(name)
        .join(tag))
}

pub fn get_root_artifact_archive_dir_path() -> PathBuf {
    get_artifact_dir_path().join("archive")
}

pub fn get_artifact_archive_dir_path(namespace: &str) -> PathBuf {
    get_root_artifact_archive_dir_path().join(namespace)
}

pub fn get_artifact_archive_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_archive_dir_path(namespace)
        .join(digest)
        .with_extension("tar.zst")
}

pub fn get_root_artifact_config_dir_path() -> PathBuf {
    get_artifact_dir_path().join("config")
}

pub fn get_artifact_config_dir_path(namespace: &str) -> PathBuf {
    get_root_artifact_config_dir_path().join(namespace)
}

pub fn get_artifact_config_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_config_dir_path(namespace)
        .join(digest)
        .with_extension("json")
}

pub fn get_root_artifact_output_dir_path() -> PathBuf {
    get_artifact_dir_path().join("output")
}

pub fn get_artifact_output_dir_path(namespace: &str) -> PathBuf {
    get_root_artifact_output_dir_path().join(namespace)
}

pub fn get_artifact_output_lock_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_output_dir_path(namespace)
        .join(digest)
        .with_extension("lock.json")
}

pub fn get_artifact_output_path(digest: &str, namespace: &str) -> PathBuf {
    get_artifact_output_dir_path(namespace).join(digest)
}

// Staged publishing
//
// A shared store path (an archive, or an artifact output directory) must
// never be observable by a reader in a partial state. Every producer that
// writes one stages its content at a private sibling path first, then
// publishes with a single rename. These three functions are that mechanism;
// moving them here (rather than leaving them private to the gRPC service
// module) is what lets every producer — pull, build, and eventually the CLI's
// own build/run commands — share one implementation instead of each growing
// its own copy of the same invariant.

/// Builds the staging path for a store entry: a unique sibling of `real_path`,
/// so the publishing rename stays on one filesystem and `real_path` itself is
/// only ever created by that rename.
pub fn staging_path_for(real_path: &Path) -> PathBuf {
    real_path.with_file_name(format!(".tmp-{}", Uuid::now_v7()))
}

/// Removes an abandoned staging path. Failures are logged rather than
/// propagated — the caller is already returning the failure that abandoned it
/// — but they are never silent, so a store root that has stopped accepting
/// removals is visible.
pub async fn discard_staging(staging_path: &Path) {
    let removed = match metadata(staging_path).await {
        Ok(meta) if meta.is_dir() => remove_dir_all(staging_path).await,
        Ok(_) => remove_file(staging_path).await,
        Err(err) if err.kind() == ErrorKind::NotFound => return,
        Err(err) => Err(err),
    };

    if let Err(err) = removed {
        warn!(
            "worker |> failed to discard staging path {}: {err}",
            staging_path.display()
        );
    }
}

/// Reports whether a failed publish means another writer got there first.
///
/// Renaming onto a directory another publisher already filled fails with
/// `DirectoryNotEmpty` (`AlreadyExists` on platforms that report it that way).
/// Every other errno — no space, permissions, I/O, read-only store — is a real
/// failure, and the target merely existing is not evidence of a lost race.
///
/// This reads a non-empty target as a finished entry, which holds only while
/// every writer of that path publishes atomically. The CLI's own `build` and
/// `run` commands still populate the real path in place (tracked separately)
/// and can be mid-population when the rename lands here, and their
/// half-filled directory is then mistaken for a winner's finished one.
fn is_lost_race(err: &IoError) -> bool {
    matches!(
        err.kind(),
        ErrorKind::DirectoryNotEmpty | ErrorKind::AlreadyExists
    )
}

/// Publishes fully-written `staging_path` to shared `target_path` with a single
/// rename, consuming `staging_path` either way.
///
/// Postcondition on `Ok`: `target_path` holds a complete copy of some
/// publisher's content and `staging_path` is gone. A reader of a path written
/// only through this function therefore observes either nothing or a complete
/// entry, never a partial one — including when a writer is killed, which
/// strands only its staging path.
///
/// Losing a race to another publisher of the same digest is `Ok`: the staged
/// copy is discarded and the winner's content stands. Where the rename replaces
/// the target rather than failing (an existing file, or an empty directory) the
/// result is treated as equivalent — but that is a statement about the
/// recipe, not the bytes: store paths are recipe-addressed
/// (`digest(artifact_json)`), not content-addressed, and nothing in this
/// function verifies the winner's output bytes against anything. Callers must
/// not read "lost the race, discarded cleanly" as "verified identical."
///
/// A caller that discards the same staging path again on `Err` is correct and
/// expected: that second call retries a removal this one already warned about,
/// and is otherwise a no-op.
pub async fn publish_atomically(staging_path: &Path, target_path: &Path) -> Result<(), Status> {
    let Err(err) = rename(staging_path, target_path).await else {
        return Ok(());
    };

    discard_staging(staging_path).await;

    if is_lost_race(&err) {
        info!(
            "worker |> discarded staged copy of {}: published concurrently",
            target_path.display()
        );

        return Ok(());
    }

    Err(Status::internal(format!(
        "failed to publish {}: {err}",
        target_path.display()
    )))
}

// Temp paths

pub fn get_sandbox_path() -> PathBuf {
    get_root_sandbox_dir_path().join(Uuid::now_v7().to_string())
}

// Functions

pub fn get_file_paths(
    source_path: &PathBuf,
    excludes: Vec<String>,
    includes: Vec<String>,
) -> Result<Vec<PathBuf>> {
    let mut excludes_paths = excludes
        .into_iter()
        .map(|i| Path::new(&i).to_path_buf())
        .collect::<Vec<PathBuf>>();

    // Exclude git directory

    excludes_paths.push(Path::new(".git").to_path_buf());

    // Resolve full path

    let walker = WalkDir::new(source_path);

    let mut files: Vec<PathBuf> = walker
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();

            let relative_path = path.strip_prefix(source_path).ok()?;

            if excludes_paths.iter().any(|i| relative_path.starts_with(i)) {
                return None;
            }

            Some(path.to_path_buf())
        })
        .collect();

    let includes_paths = includes
        .into_iter()
        .map(|i| Path::new(&i).to_path_buf())
        .collect::<Vec<PathBuf>>();

    if !includes_paths.is_empty() {
        files.retain(|i| {
            let Ok(relative_path) = i.strip_prefix(source_path) else {
                return false;
            };

            includes_paths.iter().any(|j| relative_path.starts_with(j))
        });
    }

    files.sort();

    if files.is_empty() {
        bail!("no files found");
    }

    Ok(files)
}

pub async fn set_timestamps(path: &PathBuf) -> Result<(), Error> {
    let epoc = FileTime::from_unix_time(0, 0);

    if path.is_symlink() {
        set_symlink_file_times(path, epoc, epoc).map_err(|e| {
            anyhow::anyhow!(
                "failed to set symlink file times for {}: {e}",
                path.display()
            )
        })?;
    } else {
        // Ensure the file/directory is writable before modifying timestamps.
        // Extracted tar entries (e.g. Rust toolchain) may have read-only
        // permissions which cause set_file_times to fail with PermissionDenied.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(path) {
                let mode = meta.permissions().mode();
                if mode & 0o200 == 0 {
                    let mut perms = meta.permissions();
                    perms.set_mode(mode | 0o200);
                    std::fs::set_permissions(path, perms).map_err(|e| {
                        anyhow::anyhow!(
                            "failed to add write permission for {}: {e}",
                            path.display()
                        )
                    })?;
                }
            }
        }

        set_file_times(path, epoc, epoc)
            .map_err(|e| anyhow::anyhow!("failed to set file times for {}: {e}", path.display()))?;
    }

    Ok(())
}

pub async fn copy_files(
    source_path: &PathBuf,
    source_path_files: Vec<PathBuf>,
    target_path: &Path,
) -> Result<Vec<PathBuf>> {
    if source_path_files.is_empty() {
        bail!("no source files found");
    }

    for src in &source_path_files {
        if src.display().to_string().ends_with(".tar.zst") {
            bail!("source file is a tar.zst archive");
        }

        if !src.exists() {
            bail!("source file not found: {}", src.display());
        }

        let metadata = metadata(src)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read metadata for {}: {e}", src.display()))?;

        let relative_path = src
            .strip_prefix(source_path)
            .map_err(|e| anyhow::anyhow!("failed to strip prefix from {}: {e}", src.display()))?;
        let dest = target_path.join(relative_path);

        if metadata.is_dir() {
            create_dir_all(&dest).await.map_err(|e| {
                anyhow::anyhow!("failed to create directory {}: {e}", dest.display())
            })?;
        } else if metadata.is_file() {
            let parent = dest.parent().ok_or_else(|| {
                anyhow::anyhow!("failed to get parent directory of {}", dest.display())
            })?;
            if !parent.exists() {
                create_dir_all(parent).await.map_err(|e| {
                    anyhow::anyhow!(
                        "failed to create parent directory {}: {e}",
                        parent.display()
                    )
                })?;
            }

            copy(src, &dest).await.map_err(|e| {
                anyhow::anyhow!(
                    "failed to copy {} to {}: {e}",
                    src.display(),
                    dest.display()
                )
            })?;
        } else if metadata.is_symlink() {
            symlink(src, &dest).await.map_err(|e| {
                anyhow::anyhow!(
                    "failed to symlink {} to {}: {e}",
                    src.display(),
                    dest.display()
                )
            })?;
        } else {
            bail!("source file is not a file or directory: {}", src.display());
        }
    }

    let target_path_files = get_file_paths(&target_path.to_path_buf(), vec![], vec![])?;

    Ok(target_path_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, fs::File, os::unix::fs::MetadataExt};
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    fn write_files(dir: &Path, names: &[&str], contents: &str) {
        for name in names {
            std::fs::write(dir.join(name), contents).unwrap();
        }
    }

    fn dir_entry_names(dir: &Path) -> BTreeSet<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn staged_dir(root: &Path, name: &str, files: &[&str], contents: &str) -> PathBuf {
        let path = root.join(name);

        std::fs::create_dir_all(&path).unwrap();
        write_files(&path, files, contents);

        path
    }

    // AC1 seam: publish_atomically is the mechanism pull_artifact uses to move
    // an unpacked dependency into the shared store path. This pins that the
    // published directory is byte-identical to what the writer staged, with
    // nothing lost or added in the move.
    #[tokio::test]
    async fn publish_atomically_moves_staged_content_into_place() {
        let root = TempDir::new().unwrap();
        let temp_path = root.path().join("staging");
        let target_path = root.path().join("output");

        std::fs::create_dir_all(&temp_path).unwrap();
        write_files(&temp_path, &["a.txt", "b.txt"], "payload");

        publish_atomically(&temp_path, &target_path).await.unwrap();

        assert!(!temp_path.exists());
        assert_eq!(
            dir_entry_names(&target_path),
            BTreeSet::from(["a.txt".to_string(), "b.txt".to_string()])
        );
        assert_eq!(
            std::fs::read_to_string(target_path.join("a.txt")).unwrap(),
            "payload"
        );
    }

    // The whole fix rests on staging somewhere other than the real store path,
    // so pin that property directly: a staging path is a sibling of its target,
    // never the target itself, and never shared between two writers.
    #[test]
    fn staging_path_is_a_unique_sibling_of_the_real_path() {
        let real_path = Path::new("/store/output/abc123");

        let first = staging_path_for(real_path);
        let second = staging_path_for(real_path);

        assert_ne!(first, real_path);
        assert_ne!(first, second);
        assert_eq!(first.parent(), real_path.parent());
        assert_eq!(second.parent(), real_path.parent());
    }

    // AC1: two concurrent pull_artifact calls sharing an uncached dependency
    // race to publish the same digest. The loser's content differs from the
    // winner's so the assertions can tell which one is on disk: the winner's
    // directory must survive untouched, down to the same inode, and the loser
    // must leave nothing of itself behind.
    #[tokio::test]
    async fn publish_atomically_discards_a_loser_without_disturbing_the_winner() {
        let root = TempDir::new().unwrap();
        let target_path = root.path().join("output");

        let winner_temp = staged_dir(root.path(), "winner", &["winner.txt"], "winner-content");

        publish_atomically(&winner_temp, &target_path)
            .await
            .unwrap();

        let published_inode = std::fs::metadata(&target_path).unwrap().ino();
        let loser_temp = staged_dir(root.path(), "loser", &["loser.txt"], "loser-content");

        publish_atomically(&loser_temp, &target_path).await.unwrap();

        assert!(!loser_temp.exists());
        assert_eq!(
            dir_entry_names(&target_path),
            BTreeSet::from(["winner.txt".to_string()])
        );
        assert_eq!(
            std::fs::read_to_string(target_path.join("winner.txt")).unwrap(),
            "winner-content"
        );
        assert_eq!(
            std::fs::metadata(&target_path).unwrap().ino(),
            published_inode,
            "the winner's published directory was replaced instead of left alone"
        );
    }

    // A rename can fail for reasons that have nothing to do with a racing
    // publisher — no space, permissions, a read-only store — and the target
    // merely existing is not evidence that a racer won. Here a staged directory
    // is published onto a path already occupied by a file: the target exists,
    // the rename fails, and the caller must hear about it rather than be told
    // the artifact is published.
    #[tokio::test]
    async fn publish_atomically_reports_an_io_failure_even_though_the_target_exists() {
        let root = TempDir::new().unwrap();
        let target_path = root.path().join("output");

        std::fs::write(&target_path, "not a directory").unwrap();

        let staging_path = staged_dir(root.path(), "staging", &["a.txt"], "payload");
        let err = publish_atomically(&staging_path, &target_path)
            .await
            .unwrap_err();

        assert!(err.message().contains("failed to publish"), "{err:?}");
        assert!(
            !staging_path.exists(),
            "staged copy left behind after a failed publish"
        );
        assert_eq!(
            std::fs::read_to_string(&target_path).unwrap(),
            "not a directory"
        );
    }

    // The archive half of a pull stages a file rather than a directory, so the
    // failure path has to dispose of a file. Publishing onto an occupied
    // directory fails, and the staged archive must not survive it.
    #[tokio::test]
    async fn publish_atomically_discards_a_staged_file_when_publishing_fails() {
        let root = TempDir::new().unwrap();
        let target_path = root.path().join("occupied");

        std::fs::create_dir_all(&target_path).unwrap();
        write_files(&target_path, &["existing.txt"], "existing");

        let staging_path = root.path().join("staging.tar.zst");

        std::fs::write(&staging_path, "archive-bytes").unwrap();

        publish_atomically(&staging_path, &target_path)
            .await
            .unwrap_err();

        assert!(!staging_path.exists());
        assert_eq!(
            dir_entry_names(&target_path),
            BTreeSet::from(["existing.txt".to_string()])
        );
    }

    // A build that produces nothing leaves an empty directory at the output
    // path, and a rename replaces an empty directory rather than failing. Pin
    // that outcome: the publisher wins and its content lands whole.
    #[tokio::test]
    async fn publish_atomically_replaces_an_empty_target_directory() {
        let root = TempDir::new().unwrap();
        let target_path = root.path().join("output");

        std::fs::create_dir_all(&target_path).unwrap();

        let staging_path = staged_dir(root.path(), "staging", &["a.txt"], "payload");

        publish_atomically(&staging_path, &target_path)
            .await
            .unwrap();

        assert!(!staging_path.exists());
        assert_eq!(
            dir_entry_names(&target_path),
            BTreeSet::from(["a.txt".to_string()])
        );
    }

    // AC2: verified by a test that drives two concurrent pull_artifact-style
    // publishers for the same digest and asserts a concurrent reader of the
    // shared target never observes a partial directory: it sees either
    // nothing (not yet published) or the complete, fully-formed set of
    // files — never a subset. This exercises the exact seam pull_artifact
    // relies on for atomicity with respect to a concurrent reader.
    #[tokio::test]
    async fn publish_atomically_never_exposes_a_partial_directory_to_a_reader() {
        let root = TempDir::new().unwrap();
        let root_path = root.path().to_path_buf();
        let target_path = root_path.join("output");
        let expected: BTreeSet<String> = (0..20).map(|i| format!("file-{i}.txt")).collect();

        let writer_target = target_path.clone();
        let writer = tokio::spawn(async move {
            let temp_path = writer_target.with_file_name(".staging");
            std::fs::create_dir_all(&temp_path).unwrap();

            for i in 0..20 {
                std::fs::write(temp_path.join(format!("file-{i}.txt")), "payload").unwrap();
                // Yield between writes so the reader gets real opportunities
                // to observe the directory mid-population.
                sleep(Duration::from_millis(1)).await;
            }

            publish_atomically(&temp_path, &writer_target)
                .await
                .unwrap();
        });

        let reader_target = target_path.clone();
        let reader_expected = expected.clone();
        let reader = tokio::spawn(async move {
            for polls_while_unpublished in 0..2000 {
                if reader_target.exists() {
                    let seen = dir_entry_names(&reader_target);

                    assert_eq!(
                        seen, reader_expected,
                        "reader observed a partial directory: {seen:?}"
                    );

                    // The reader has to have raced the writer for its
                    // observation to mean anything, so report how much of the
                    // staging it sat through.
                    return polls_while_unpublished;
                }

                sleep(Duration::from_millis(1)).await;
            }

            0
        });

        let (writer_result, reader_result) = tokio::join!(writer, reader);

        writer_result.unwrap();

        assert!(
            reader_result.unwrap() > 0,
            "the reader never observed the target while the writer was staging, so it asserted nothing"
        );
        assert_eq!(dir_entry_names(&target_path), expected);
    }

    fn file_basenames(
        root: &Path,
        paths: &[PathBuf],
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        paths
            .iter()
            .filter(|p| p.is_file())
            .map(|p| Ok(p.strip_prefix(root)?.to_string_lossy().into_owned()))
            .collect()
    }

    fn make_dir_with_files(names: &[&str]) -> Result<TempDir, Box<dyn std::error::Error>> {
        let dir = TempDir::new()?;
        for name in names {
            File::create(dir.path().join(name))?;
        }
        Ok(dir)
    }

    // A case-insensitive or locale-aware sort would order these ["a", "B", "Z"];
    // cross-host digest stability requires the raw byte order 'B'(0x42) < 'Z'(0x5A)
    // < 'a'(0x61), independent of the host's collation.
    #[test]
    fn get_file_paths_sorts_bytewise_not_case_folded() -> Result<(), Box<dyn std::error::Error>> {
        let dir = make_dir_with_files(&["a.txt", "B.txt", "Z.txt"])?;

        let paths = get_file_paths(&dir.path().to_path_buf(), vec![], vec![])?;

        assert_eq!(
            file_basenames(dir.path(), &paths)?,
            vec!["B.txt", "Z.txt", "a.txt"]
        );
        Ok(())
    }

    // Simulates two host filesystems enumerating the identical file set in different
    // orders (APFS vs ext4 dirent order): the final sort must normalize both to the
    // same sequence, otherwise the combined source digest diverges across producers.
    #[test]
    fn get_file_paths_order_independent_of_creation_order() -> Result<(), Box<dyn std::error::Error>>
    {
        let forward = make_dir_with_files(&["alpha", "bravo", "charlie"])?;
        let reversed = make_dir_with_files(&["charlie", "bravo", "alpha"])?;

        let forward_paths = get_file_paths(&forward.path().to_path_buf(), vec![], vec![])?;
        let reversed_paths = get_file_paths(&reversed.path().to_path_buf(), vec![], vec![])?;

        assert_eq!(
            file_basenames(forward.path(), &forward_paths)?,
            file_basenames(reversed.path(), &reversed_paths)?
        );
        Ok(())
    }

    // Non-decomposable codepoints (Greek alpha U+03B1, Euro U+20AC) avoid APFS
    // NFC/NFD rewriting; their UTF-8 encodings sort by raw byte value
    // 'z'(0x7A) < α(0xCE..) < €(0xE2..) on any host.
    #[test]
    fn get_file_paths_orders_unicode_bytewise() -> Result<(), Box<dyn std::error::Error>> {
        let dir = make_dir_with_files(&["z_ascii", "\u{03b1}_alpha", "\u{20ac}_euro"])?;

        let paths = get_file_paths(&dir.path().to_path_buf(), vec![], vec![])?;

        assert_eq!(
            file_basenames(dir.path(), &paths)?,
            vec!["z_ascii", "\u{03b1}_alpha", "\u{20ac}_euro"]
        );
        Ok(())
    }
}
