use anyhow::{anyhow, bail, Error, Result};
use filetime::{set_file_times, set_symlink_file_times, FileTime};
use std::{
    io::{Error as IoError, ErrorKind},
    path::{Path, PathBuf},
};
use tokio::fs::{
    copy, create_dir_all, metadata, remove_dir_all, remove_file, rename, symlink, symlink_metadata,
};
use tracing::{info, warn};
use uuid::Uuid;
use vorpal_sdk::api::artifact::ArtifactSystem;
use walkdir::WalkDir;

// Root paths
//
// Re-exported from the SDK rather than defined twice. The credentials path
// itself is gone from here entirely: the CLI now writes and reads that file
// through the SDK, and two definitions drifting would have login write one
// path while every reader looks at another.
pub use vorpal_sdk::context::{get_root_dir_path, get_root_key_dir_path};

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

// Store path components
//
// The functions above join a digest and a namespace straight into a store
// path, and every one of those values reaches this process from somewhere else
// — a gRPC request, a registry response, an alias file on disk. A value that
// is not a single, inert path component therefore lets whoever supplied it
// choose a destination outside the store. Both shapes are parsed here, beside
// the joins they protect, so there is one rule rather than one per caller.

/// Length of a sha256 digest in lowercase hex, the only shape a store path
/// component ever takes (`sdk/rust/src/context.rs` hashes artifact JSON with
/// `sha256::digest`).
const ARTIFACT_DIGEST_LENGTH: usize = 64;

/// Parses a digest naming a store entry. `source` names where the value came
/// from, so a refusal says which input was hostile.
pub fn parse_artifact_digest(digest: &str, source: &str) -> Result<String> {
    let is_lowercase_hex = digest
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));

    if digest.len() != ARTIFACT_DIGEST_LENGTH || !is_lowercase_hex {
        bail!(
            "invalid artifact digest from {source}: expected {ARTIFACT_DIGEST_LENGTH} lowercase \
             hex characters, got {:?}",
            digest,
        );
    }

    Ok(digest.to_string())
}

/// Parses a value that names exactly one directory under a store root — a
/// namespace, an alias name, a tag. A value carrying a separator, or equal to
/// `.` or `..`, escapes that root the same way a hostile digest would, and one
/// wearing the staging prefix is indistinguishable from a staging sibling.
///
/// Returns the component rather than `()` so the checked value is what callers
/// go on to use: a call site that drops the parse stops compiling instead of
/// quietly joining the unchecked string into a store path.
///
/// Bounded in length to the same ceiling as [`parse_alias_name`]: the
/// registry's archive-check cache keys a bounded-entry-count cache on
/// `"{namespace}/{digest}"` (`registry.rs`'s `check_cache`), so an unbounded
/// namespace or tag length was still unbounded per-entry memory even after
/// the entry-count cap landed (VPL-383 CLUSTER-17).
pub fn parse_store_path_component(value: &str, field: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > ARTIFACT_ALIAS_NAME_LENGTH
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.starts_with(STAGING_PREFIX)
    {
        bail!(
            "invalid artifact {field} {:?}: must be non-empty, at most \
             {ARTIFACT_ALIAS_NAME_LENGTH} characters, contain no path separator or NUL byte, not \
             be '.' or '..', and not start with the reserved staging prefix {:?}",
            value,
            STAGING_PREFIX,
        );
    }

    Ok(value.to_string())
}

/// Longest alias name the registry will write, matching the bound
/// `store_artifact` has enforced on write since it was introduced.
const ARTIFACT_ALIAS_NAME_LENGTH: usize = 255;

/// Parses an artifact alias name: the naming policy `store_artifact` already
/// enforces on write, now the one place both a write (`store_artifact`) and a
/// read (`get_artifact_alias`) call to check a name before it is joined into
/// a store path. Stricter than [`parse_store_path_component`] because an
/// alias name is also a user-facing identifier, not merely a containment
/// boundary: no leading/trailing `.`/`-`, no whitespace, and an
/// alphanumeric/`_`/`-`/`.` allowlist rather than a denylist.
pub fn parse_alias_name(name: &str, field: &str) -> Result<String> {
    let is_allowed_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.';

    if name.is_empty()
        || name.len() > ARTIFACT_ALIAS_NAME_LENGTH
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name.starts_with('.')
        || name.ends_with('.')
        || name.starts_with('-')
        || name.ends_with('-')
        || name.chars().any(char::is_whitespace)
        || !name.chars().all(is_allowed_char)
    {
        bail!(
            "invalid artifact {field} {:?}: must be non-empty, at most {ARTIFACT_ALIAS_NAME_LENGTH} \
             characters, contain only alphanumeric characters, '_', '-', and '.', and not start \
             or end with '.' or '-'",
            name,
        );
    }

    Ok(name.to_string())
}

/// Splits an alias string of the form `"name"` or `"name:tag"` into its name
/// and tag, defaulting the tag to `"latest"` when absent. The single home for
/// this split: `store_artifact`'s handler and both `ArtifactBackend`
/// implementations (`local.rs`, `s3.rs`) each used to keep their own copy, so
/// a validated `(name, tag)` pair and the pair a backend actually joined into
/// a path could drift apart if one copy changed and the others did not.
pub fn split_alias_name_tag(alias: &str) -> (&str, &str) {
    let mut parts = alias.split(':');
    // `str::split` always yields at least one item, even for `""` — the
    // `unwrap_or(alias)` this replaced could never take its fallback arm
    // (VPL-383 CLUSTER-20); `unwrap_or_default()` names the same guaranteed
    // case honestly instead of carrying dead code that implies a path that
    // does not exist.
    let name = parts.next().unwrap_or_default();
    let tag = parts.next().unwrap_or("latest");

    (name, tag)
}

// Staged publishing
//
// A shared store path (an archive, or an artifact output directory) must
// never be observable by a reader in a partial state. Every producer that
// writes one stages its content at a private sibling path first, then
// publishes with a single rename. These three functions are that mechanism,
// and they live here rather than private to the gRPC service module so that
// every producer — the worker's pull and build paths, and the CLI's own
// build/run commands — shares one implementation instead of each growing its
// own copy of the same invariant.

/// The name every staging path starts with, reserved so no store entry can
/// wear it (`parse_store_path_component`).
pub const STAGING_PREFIX: &str = ".tmp-";

/// Builds the staging path for a store entry: a unique sibling of `real_path`,
/// so the publishing rename stays on one filesystem and `real_path` itself is
/// only ever created by that rename.
pub fn staging_path_for(real_path: &Path) -> PathBuf {
    real_path.with_file_name(format!("{STAGING_PREFIX}{}", Uuid::now_v7()))
}

/// Removes an abandoned staging path. Failures are logged rather than
/// propagated — the caller is already returning the failure that abandoned it
/// — but they are never silent, so a store root that has stopped accepting
/// removals is visible.
///
/// The file-or-directory decision is made on the link itself, not on what it
/// points at: a build step owns the staging directory while it runs and can
/// replace it with a symlink, and a following stat would then send
/// `remove_dir_all` at the symlink's target instead of at the link.
pub async fn discard_staging(staging_path: &Path) {
    let removed = match symlink_metadata(staging_path).await {
        Ok(meta) if meta.is_dir() => remove_dir_all(staging_path).await,
        Ok(_) => remove_file(staging_path).await,
        Err(err) if err.kind() == ErrorKind::NotFound => return,
        Err(err) => Err(err),
    };

    if let Err(err) = removed {
        warn!(
            "store |> failed to discard staging path {}: {err}",
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
/// every writer of that path publishes atomically. Every producer in this
/// repository now does — the worker's pull and build paths and the CLI's
/// `build` and `run` commands all stage and rename through this function — so
/// a non-empty target is another publisher's completed rename. A future
/// producer that populates a store path in place breaks that reading, and its
/// half-filled directory would be mistaken here for a winner's finished one.
fn is_lost_race(err: &IoError) -> bool {
    matches!(
        err.kind(),
        ErrorKind::DirectoryNotEmpty | ErrorKind::AlreadyExists
    )
}

/// Which publisher's bytes stand at the target path once a publish succeeds.
///
/// The distinction matters because it is not observable from the path
/// afterwards: store paths are recipe-addressed (`digest(artifact_json)`), not
/// content-addressed, so two publishers of one digest can hold genuinely
/// different bytes and the loser has no way to notice. A caller that already
/// shipped its own copy somewhere else — pushed an archive to a registry, say
/// — needs to know that the copy it shipped is not the one in the store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishOutcome {
    /// This caller's staged content is what now stands at the target path.
    Published,
    /// Another publisher of the same path got there first. This caller's
    /// staged copy was discarded and the target holds the winner's bytes.
    Superseded,
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
/// Losing a race to another publisher of the same digest is `Ok`, reported as
/// `Superseded`: the staged copy is discarded and the winner's content stands.
/// Where the rename replaces the target rather than failing (an existing file,
/// or an empty directory) this caller won, and the result is `Published`.
/// Neither outcome says anything about the bytes — nothing in this function
/// verifies any publisher's content against anything — so callers must not read
/// `Superseded` as "verified identical."
///
/// A caller that discards the same staging path again on `Err` is correct and
/// expected: that second call retries a removal this one already warned about,
/// and is otherwise a no-op.
pub async fn publish_atomically(staging_path: &Path, target_path: &Path) -> Result<PublishOutcome> {
    let Err(err) = rename(staging_path, target_path).await else {
        return Ok(PublishOutcome::Published);
    };

    discard_staging(staging_path).await;

    if is_lost_race(&err) {
        info!(
            "store |> discarded staged copy of {}: published concurrently",
            target_path.display()
        );

        return Ok(PublishOutcome::Superseded);
    }

    Err(anyhow!(
        "failed to publish {}: {err}",
        target_path.display()
    ))
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

    // The seam every producer publishes through — the worker's pull and build
    // paths and the CLI's build/run commands. This pins that the published
    // directory is byte-identical to what the writer staged, with nothing lost
    // or added in the move.
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

    // Two producers of the same digest race to publish it. The loser's content
    // differs from the winner's so the assertions can tell which one is on
    // disk: the winner's directory must survive untouched, down to the same
    // inode, the loser must leave nothing of itself behind, and the loser must
    // be told it was superseded rather than that it published.
    #[tokio::test]
    async fn publish_atomically_discards_a_loser_without_disturbing_the_winner() {
        let root = TempDir::new().unwrap();
        let target_path = root.path().join("output");

        let winner_temp = staged_dir(root.path(), "winner", &["winner.txt"], "winner-content");

        assert_eq!(
            publish_atomically(&winner_temp, &target_path)
                .await
                .unwrap(),
            PublishOutcome::Published
        );

        let published_inode = std::fs::metadata(&target_path).unwrap().ino();
        let loser_temp = staged_dir(root.path(), "loser", &["loser.txt"], "loser-content");

        assert_eq!(
            publish_atomically(&loser_temp, &target_path).await.unwrap(),
            PublishOutcome::Superseded,
            "the loser was told it published its own bytes"
        );

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

        assert!(err.to_string().contains("failed to publish"), "{err:?}");
        assert!(
            !staging_path.exists(),
            "staged copy left behind after a failed publish"
        );
        assert_eq!(
            std::fs::read_to_string(&target_path).unwrap(),
            "not a directory"
        );
    }

    // A build step owns its staging directory while it runs, so it can replace
    // that directory with a symlink and then fail. Discarding must delete the
    // link the producer staged, never walk through it into whatever it points
    // at — the pointed-at directory here stands in for anything else on the
    // host the worker can write.
    #[tokio::test]
    async fn discard_staging_removes_a_symlinked_staging_path_not_its_target() {
        let root = TempDir::new().unwrap();
        let elsewhere = staged_dir(root.path(), "elsewhere", &["keep.txt"], "keep");
        let staging_path = root.path().join(".tmp-staging");

        tokio::fs::symlink(&elsewhere, &staging_path).await.unwrap();

        discard_staging(&staging_path).await;

        assert!(
            !staging_path.exists(),
            "the staged symlink survived discarding"
        );
        assert_eq!(
            dir_entry_names(&elsewhere),
            BTreeSet::from(["keep.txt".to_string()]),
            "discarding followed the symlink and deleted its target"
        );
    }

    // Producers that publish an archive or an alias stage a file rather than a
    // directory, so the failure path has to dispose of a file. Publishing onto
    // an occupied directory fails, and the staged file must not survive it.
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

    // A rename onto an empty directory replaces it rather than failing, so an
    // empty target is not a racer this publisher lost to. Pin that outcome:
    // the publisher wins, its content lands whole, and it is told it won.
    //
    // Publishing nothing is refused by `stage_then_publish` in the worker, so
    // the refusal binds the worker's producers only — the CLI's `build` and
    // `run` write these paths without it.
    #[tokio::test]
    async fn publish_atomically_replaces_an_empty_target_directory() {
        let root = TempDir::new().unwrap();
        let target_path = root.path().join("output");

        std::fs::create_dir_all(&target_path).unwrap();

        let staging_path = staged_dir(root.path(), "staging", &["a.txt"], "payload");

        assert_eq!(
            publish_atomically(&staging_path, &target_path)
                .await
                .unwrap(),
            PublishOutcome::Published
        );

        assert!(!staging_path.exists());
        assert_eq!(
            dir_entry_names(&target_path),
            BTreeSet::from(["a.txt".to_string()])
        );
    }

    // The property every reader in the store depends on: a reader polling the
    // shared target while a publisher stages it sees either nothing (not yet
    // published) or the complete, fully-formed set of files — never a subset.
    // Readers test readiness with a bare `exists()`, so a partial directory
    // appearing here would be taken for a finished entry.
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

    // -------------------------------------------------------------------
    // Store path component parsers (VPL-383): these guard every value a
    // gRPC caller supplies before it is joined into a store path, so their
    // own rejection shapes need to be pinned directly rather than only
    // through the registry handlers that call them.
    // -------------------------------------------------------------------

    #[test]
    fn parse_store_path_component_rejects_a_nul_byte() {
        let result = parse_store_path_component("a\0b", "tag");

        assert!(result.is_err());
    }

    #[test]
    fn parse_store_path_component_accepts_a_well_formed_value() {
        let result = parse_store_path_component("library", "namespace");

        assert_eq!(result.unwrap(), "library");
    }

    #[test]
    fn parse_store_path_component_rejects_a_value_over_the_length_bound() {
        // VPL-383 CLUSTER-17: an unbounded namespace/tag length kept the
        // archive-check cache's per-entry memory unbounded even after its
        // entry count was capped, because the cache key is built from these
        // components directly.
        let too_long = "a".repeat(ARTIFACT_ALIAS_NAME_LENGTH + 1);

        let result = parse_store_path_component(&too_long, "namespace");

        assert!(result.is_err());
    }

    #[test]
    fn parse_store_path_component_accepts_a_value_at_the_length_bound() {
        let at_bound = "a".repeat(ARTIFACT_ALIAS_NAME_LENGTH);

        let result = parse_store_path_component(&at_bound, "namespace");

        assert_eq!(result.unwrap(), at_bound);
    }

    #[test]
    fn parse_store_path_component_error_names_the_field() {
        // The refusal must say *which input* was hostile. (VPL-383
        // CLUSTER-13: this test previously also asserted the message
        // excludes "/var/lib/vorpal" and "store/artifact" — properties that
        // cannot fail as long as this function's `bail!` format string is
        // unchanged, since the function never calls a path builder or reads
        // a root path to begin with; those assertions pinned nothing. The
        // property that actually matters — that a rejected value never
        // reaches a path join — is proven at the call site, not here: see
        // `registry.rs`'s `test_check_rejects_a_traversing_digest` and its
        // siblings, which assert the mock backend's call count stays 0.)
        let err = parse_store_path_component("a/b", "namespace")
            .unwrap_err()
            .to_string();

        assert!(err.contains("namespace"));
    }

    #[test]
    fn parse_alias_name_rejects_a_nul_byte() {
        let result = parse_alias_name("a\0b", "alias name");

        assert!(result.is_err());
    }

    #[test]
    fn parse_alias_name_rejects_a_path_separator() {
        assert!(parse_alias_name("../etc/passwd", "alias name").is_err());
        assert!(parse_alias_name("a/b", "alias name").is_err());
        assert!(parse_alias_name("a\\b", "alias name").is_err());
    }

    #[test]
    fn parse_alias_name_rejects_leading_or_trailing_dot_or_dash() {
        assert!(parse_alias_name(".rust", "alias name").is_err());
        assert!(parse_alias_name("rust.", "alias name").is_err());
        assert!(parse_alias_name("-rust", "alias name").is_err());
        assert!(parse_alias_name("rust-", "alias name").is_err());
    }

    #[test]
    fn parse_alias_name_rejects_whitespace_and_disallowed_characters() {
        assert!(parse_alias_name("rust 1", "alias name").is_err());
        assert!(parse_alias_name("rust!", "alias name").is_err());
        assert!(parse_alias_name("", "alias name").is_err());
    }

    #[test]
    fn parse_alias_name_rejects_a_name_over_the_length_bound() {
        let long_name = "a".repeat(ARTIFACT_ALIAS_NAME_LENGTH + 1);

        assert!(parse_alias_name(&long_name, "alias name").is_err());
    }

    #[test]
    fn parse_alias_name_accepts_a_well_formed_name() {
        let result = parse_alias_name("rust_1.85-nightly", "alias name");

        assert_eq!(result.unwrap(), "rust_1.85-nightly");
    }

    #[test]
    fn parse_alias_name_error_names_the_field() {
        let err = parse_alias_name("a/b", "artifact alias name")
            .unwrap_err()
            .to_string();

        assert!(err.contains("artifact alias name"));
    }

    #[test]
    fn split_alias_name_tag_splits_name_and_tag() {
        assert_eq!(split_alias_name_tag("rust:1.85"), ("rust", "1.85"));
    }

    #[test]
    fn split_alias_name_tag_defaults_tag_to_latest() {
        assert_eq!(split_alias_name_tag("rust"), ("rust", "latest"));
    }

    #[test]
    fn split_alias_name_tag_ignores_a_third_colon_delimited_segment() {
        // Matches the pre-existing behavior every caller relied on: only the
        // first two colon-delimited segments are meaningful.
        assert_eq!(split_alias_name_tag("rust:1.85:extra"), ("rust", "1.85"));
    }
}

/// Store-root override tests.
///
/// `VORPAL_ROOT_PATH` is process-global state that every path builder above
/// reads, so these tests serialize on [`ROOT_PATH_ENV_LOCK`]. Any future test
/// that sets the variable, or that asserts on a value derived from
/// `get_root_dir_path`, must take the same lock.
#[cfg(test)]
mod root_path_override_tests {
    use super::*;
    use tempfile::TempDir;

    static ROOT_PATH_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Sets `VORPAL_ROOT_PATH` to a fresh temporary directory for the caller's
    /// lifetime and restores the previous value on drop, so the variable never
    /// leaks into another test or into the rest of the process.
    struct ScratchRoot {
        _guard: std::sync::MutexGuard<'static, ()>,
        previous: Option<String>,
        dir: TempDir,
    }

    impl ScratchRoot {
        fn new() -> Self {
            let guard = ROOT_PATH_ENV_LOCK
                .lock()
                .unwrap_or_else(|err| err.into_inner());

            let previous = std::env::var("VORPAL_ROOT_PATH").ok();
            let dir = TempDir::new().unwrap();

            std::env::set_var("VORPAL_ROOT_PATH", dir.path());

            Self {
                _guard: guard,
                previous,
                dir,
            }
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    impl Drop for ScratchRoot {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var("VORPAL_ROOT_PATH", value),
                None => std::env::remove_var("VORPAL_ROOT_PATH"),
            }
        }
    }

    /// Holds `VORPAL_ROOT_PATH` at a chosen raw value, including an empty one,
    /// which `ScratchRoot` cannot express.
    struct RawRoot {
        _guard: std::sync::MutexGuard<'static, ()>,
        previous: Option<String>,
    }

    impl RawRoot {
        fn new(value: &str) -> Self {
            let guard = ROOT_PATH_ENV_LOCK
                .lock()
                .unwrap_or_else(|err| err.into_inner());

            let previous = std::env::var("VORPAL_ROOT_PATH").ok();

            std::env::set_var("VORPAL_ROOT_PATH", value);

            Self {
                _guard: guard,
                previous,
            }
        }
    }

    impl Drop for RawRoot {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var("VORPAL_ROOT_PATH", value),
                None => std::env::remove_var("VORPAL_ROOT_PATH"),
            }
        }
    }

    // The whole point of the override: every path a registry serves must land
    // under the scratch root, so a verification instance can run without
    // touching the operator's real `/var/lib/vorpal`.
    #[test]
    fn every_store_path_is_rooted_at_the_override() {
        let root = ScratchRoot::new();

        let paths = [
            get_root_dir_path(),
            get_root_key_dir_path(),
            get_root_store_dir_path(),
            get_root_sandbox_dir_path(),
            get_artifact_dir_path(),
            get_root_artifact_alias_dir_path(),
            get_root_artifact_archive_dir_path(),
            get_root_artifact_config_dir_path(),
            get_root_artifact_output_dir_path(),
            get_artifact_archive_path(&"a".repeat(64), "library"),
            get_artifact_config_path(&"a".repeat(64), "library"),
            get_artifact_output_path(&"a".repeat(64), "library"),
            get_artifact_alias_path("rust", "library", ArtifactSystem::Aarch64Linux, "latest")
                .unwrap(),
            get_key_ca_key_path(),
            get_key_service_path(),
        ];

        for path in paths {
            assert!(
                path.starts_with(root.path()),
                "{} escaped the scratch root {}",
                path.display(),
                root.path().display()
            );
        }

        assert!(
            !get_root_dir_path().starts_with("/var/lib/vorpal"),
            "the override was ignored in favor of the built-in default"
        );
    }

    // An unset variable must leave the operator's real store exactly where it
    // has always been; the override is opt-in, not a relocation.
    #[test]
    fn an_unset_override_keeps_the_default_root() {
        let _guard = ROOT_PATH_ENV_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());

        let previous = std::env::var("VORPAL_ROOT_PATH").ok();

        std::env::remove_var("VORPAL_ROOT_PATH");

        let observed = get_root_dir_path();

        match previous {
            Some(value) => std::env::set_var("VORPAL_ROOT_PATH", value),
            None => {}
        }

        assert_eq!(observed, Path::new("/var/lib/vorpal"));
    }

    // `VORPAL_ROOT_PATH=` is what a shell leaves behind when an operator
    // clears the variable rather than unsetting it. Treating that empty string
    // as a root would put the whole store at `/store`, `/key` and `/sandbox`,
    // so it must fall back to the default exactly as `get_socket_path` does.
    #[test]
    fn an_empty_override_falls_back_to_the_default_root() {
        let _root = RawRoot::new("");

        assert_eq!(get_root_dir_path(), Path::new("/var/lib/vorpal"));
    }

    // The socket lives under the root too, so pointing the root at a scratch
    // directory moves the socket with it — otherwise a verification instance
    // would still try to bind inside the operator's real store.
    #[test]
    fn the_socket_path_follows_the_override_when_its_own_variable_is_unset() {
        let root = ScratchRoot::new();

        let previous_socket = std::env::var("VORPAL_SOCKET_PATH").ok();

        std::env::remove_var("VORPAL_SOCKET_PATH");

        let observed = get_socket_path();

        if let Some(value) = previous_socket {
            std::env::set_var("VORPAL_SOCKET_PATH", value);
        }

        assert_eq!(observed, root.path().join("vorpal.sock"));
    }
}
