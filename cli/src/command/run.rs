use crate::command::store::{
    archives::unpack_zstd,
    paths::{
        discard_staging, get_artifact_alias_path, get_artifact_archive_path,
        get_artifact_output_path, get_file_paths, publish_atomically, set_timestamps,
        staging_path_for,
    },
};
use anyhow::{anyhow, bail, Context, Result};
use std::fmt::Write as _;
use std::{
    os::unix::{ffi::OsStrExt, fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};
use tokio::{
    fs::{create_dir_all, read_link, read_to_string, rename, symlink_metadata, write, File},
    io::{AsyncReadExt, BufReader},
};
use tonic::{Code, Request};
use tracing::{debug, info};
use vorpal_sdk::{
    api::{
        archive::{archive_service_client::ArchiveServiceClient, ArchivePullRequest},
        artifact::{
            artifact_service_client::ArtifactServiceClient, ArtifactSystem, GetArtifactAliasRequest,
        },
    },
    artifact::system::get_system_default,
    context::{build_channel, client_auth_header, parse_artifact_alias},
};

/// Length of a sha256 digest in lowercase hex, the only shape a store path
/// component ever takes (`sdk/rust/src/context.rs` hashes artifact JSON with
/// `sha256::digest`).
const ARTIFACT_DIGEST_LENGTH: usize = 64;

/// A digest is joined straight into a store path by
/// `get_artifact_output_path` and `get_artifact_archive_path`, and the
/// directory it names is executed from. Anything other than a bare sha256
/// hex string therefore lets whoever supplied the digest — a registry, or
/// the alias file on disk — choose a destination outside the store, so the
/// shape is checked at every point the value enters this process rather than
/// trusted from its source.
fn parse_artifact_digest(digest: &str, source: &str) -> Result<String> {
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

async fn get_alias_from_registry(
    registry: &str,
    name: &str,
    namespace: &str,
    system: ArtifactSystem,
    tag: &str,
) -> Result<String> {
    let client_channel = build_channel(registry).await?;
    let mut client = ArtifactServiceClient::new(client_channel);

    let request = GetArtifactAliasRequest {
        system: system.into(),
        name: name.to_string(),
        namespace: namespace.to_string(),
        tag: tag.to_string(),
    };

    let mut request = Request::new(request);
    let request_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {e}"))?;

    if let Some(header) = request_auth_header {
        request.metadata_mut().insert("authorization", header);
    }

    let response = client.get_artifact_alias(request).await.map_err(|status| {
        if status.code() == Code::NotFound {
            anyhow!("alias not found in registry")
        } else {
            anyhow!("registry error: {status:?}")
        }
    })?;

    let digest = response.into_inner().digest;

    if digest.is_empty() {
        bail!("registry returned empty digest for alias");
    }

    parse_artifact_digest(&digest, "registry")
}

async fn read_alias_digest(alias_path: &Path, artifact_name: &str) -> Result<String> {
    let artifact_digest = read_to_string(alias_path)
        .await
        .with_context(|| format!("failed to read alias file: {}", alias_path.display()))?;

    let artifact_digest = artifact_digest.trim().to_string();

    if artifact_digest.is_empty() {
        bail!(
            "alias file is empty: {}\n\
             \n\
             The alias file exists but contains no digest. Try rebuilding:\n\
             \n\
             \tvorpal build {}",
            alias_path.display(),
            artifact_name,
        );
    }

    parse_artifact_digest(&artifact_digest, "alias file")
}

/// `parse_artifact_alias`'s own component check (`sdk/rust/src/context.rs`,
/// `is_valid_component`) allows `.`, so a namespace of `..` passes it and
/// resolves one level up when joined into a store path. That is not the
/// traversal the caller was trying to allow, so it is rejected here too: a
/// stricter, redundant check at the one place this process's own store paths
/// are built from the alias's namespace.
fn parse_artifact_namespace(namespace: &str) -> Result<()> {
    if namespace == "." || namespace == ".." {
        bail!(
            "invalid artifact namespace {:?}: must not be '.' or '..'",
            namespace,
        );
    }

    Ok(())
}

/// The prefix every staging path is named with (`staging_path_for`,
/// `store/paths.rs`). This producer's corpus is archive content from
/// whoever the registry forwards, not content this process staged itself, so
/// an archive that embeds this prefix hands whoever crafted it a path a
/// concurrent build's own staging traffic will pass through once this
/// producer's rename retires the directory the archive named. Unlike the
/// worker's own embedded-reference scan (which matches the one staging name
/// a single build used), this check matches the general prefix.
const STAGING_PATH_NEEDLE: &[u8] = b".tmp-";

const STAGING_SCAN_CHUNK_SIZE: usize = 8192;

/// Scans every regular file and symlink target under `staged_files` for
/// `STAGING_PATH_NEEDLE`, in overlapping chunks so a match straddling a read
/// boundary is still found. Returns the first offending path, if any.
async fn find_staging_path_reference(staged_files: &[PathBuf]) -> Result<Option<PathBuf>> {
    let needle = STAGING_PATH_NEEDLE;
    let overlap = needle.len() - 1;

    for path in staged_files.iter() {
        let metadata = symlink_metadata(path)
            .await
            .map_err(|err| anyhow!("failed to stat staged file {}: {err}", path.display()))?;

        if metadata.is_symlink() {
            let target = read_link(path)
                .await
                .map_err(|err| anyhow!("failed to read staged link {}: {err}", path.display()))?;

            if target
                .as_os_str()
                .as_bytes()
                .windows(needle.len())
                .any(|w| w == needle)
            {
                return Ok(Some(path.clone()));
            }

            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let mut reader = BufReader::new(
            File::open(path)
                .await
                .map_err(|err| anyhow!("failed to open staged file {}: {err}", path.display()))?,
        );

        let mut buf = vec![0u8; STAGING_SCAN_CHUNK_SIZE + overlap];
        let mut carried = 0usize;

        loop {
            let read = reader
                .read(&mut buf[carried..])
                .await
                .map_err(|err| anyhow!("failed to read staged file {}: {err}", path.display()))?;

            if read == 0 {
                break;
            }

            let filled = carried + read;

            if buf[..filled].windows(needle.len()).any(|w| w == needle) {
                return Ok(Some(path.clone()));
            }

            carried = overlap.min(filled);
            buf.copy_within(filled - carried..filled, 0);
        }
    }

    Ok(None)
}

/// Writes `data` to a staged sibling of `archive_path`, then publishes it
/// with a single rename onto the shared store path. No reader of
/// `archive_path` ever observes a partial or truncated file — mirrors the
/// worker's own publish path (`cli/src/command/start/worker.rs`).
async fn publish_archive_bytes(data: &[u8], archive_path: &Path) -> Result<()> {
    if let Some(parent) = archive_path.parent() {
        create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let staging_path = staging_path_for(archive_path);

    let staged: Result<()> = async {
        write(&staging_path, data)
            .await
            .with_context(|| format!("failed to write: {}", staging_path.display()))?;

        set_timestamps(&staging_path).await?;

        publish_atomically(&staging_path, archive_path)
            .await
            .map(|_| ())
    }
    .await;

    if staged.is_err() {
        discard_staging(&staging_path).await;
    }

    staged
}

/// Unpacks `archive_path` into a staged sibling of `output_path`, then
/// publishes it with a single rename, so the real output path is only ever
/// created — whole — by that rename. Mirrors the worker's own publish path.
async fn publish_unpacked_output(archive_path: &Path, output_path: &Path) -> Result<()> {
    let staging_path = staging_path_for(output_path);

    create_dir_all(&staging_path)
        .await
        .with_context(|| format!("failed to create staging path: {}", staging_path.display()))?;

    let staged: Result<()> = async {
        unpack_zstd(&staging_path, archive_path).await?;

        let staged_files = get_file_paths(&staging_path.to_path_buf(), vec![], vec![])?;

        // `get_file_paths` walks from the staging root down and reports every
        // entry, so its result is never empty for a directory that exists and
        // counting it says nothing about content: an archive of no entries, or
        // of directory entries only, unpacks to a tree with no regular file in
        // it. Publishing that leaves a store path every later `exists()` check
        // reads as a finished artifact and no reader can use, so refuse before
        // the rename. This is a smoke test for "the unpack produced something",
        // never evidence the tree is the intended one — the archive bytes are
        // not verified against the digest anywhere.
        if !staged_files.iter().any(|path| path.is_file()) {
            // The archive is the unusable input, so retire it too: otherwise
            // the `archive_path.exists()` check short-circuits the pull on
            // every later invocation and this digest can never be run again.
            retire_atomically(archive_path).await?;

            bail!("archive unpacked no files: {}", archive_path.display());
        }

        if let Some(offender) = find_staging_path_reference(&staged_files).await? {
            // As with the emptiness bail above, the archive is the unusable
            // input: retire it so the next invocation re-pulls instead of
            // short-circuiting on a cached archive already rejected.
            retire_atomically(archive_path).await?;

            let offender = offender
                .strip_prefix(&staging_path)
                .unwrap_or(&offender)
                .display()
                .to_string();

            bail!(
                "archive embeds a staging-path reference in {offender}, which does not survive \
                 publishing"
            );
        }

        for path in staged_files.iter() {
            set_timestamps(path).await?;
        }

        publish_atomically(&staging_path, output_path)
            .await
            .map(|_| ())
    }
    .await;

    if staged.is_err() {
        discard_staging(&staging_path).await;
    }

    staged
}

/// Retires `real_path` (file or directory) so a concurrent reader observes
/// the whole entry or nothing, never a partial removal mid-`remove_dir_all`:
/// renames it onto a staging sibling, then discards the staged copy.
/// Retiring a path that is already gone is a no-op, not an error.
async fn retire_atomically(real_path: &Path) -> Result<()> {
    if !real_path.exists() {
        return Ok(());
    }

    let staging_path = staging_path_for(real_path);

    rename(real_path, &staging_path)
        .await
        .map_err(|err| anyhow!("failed to retire {}: {err}", real_path.display()))?;

    discard_staging(&staging_path).await;

    Ok(())
}

async fn pull_artifact_from_registry(
    registry: &str,
    digest: &str,
    namespace: &str,
    output_path: &std::path::Path,
) -> Result<()> {
    let archive_path = get_artifact_archive_path(digest, namespace);

    if !archive_path.exists() {
        let client_channel = build_channel(registry).await?;
        let mut client_archive = ArchiveServiceClient::new(client_channel);

        // Pull archive from registry

        let request = ArchivePullRequest {
            digest: digest.to_string(),
            namespace: namespace.to_string(),
        };

        let mut request = Request::new(request);
        let request_auth_header = client_auth_header(registry)
            .await
            .map_err(|e| anyhow!("failed to get client auth header: {e}"))?;

        if let Some(header) = request_auth_header {
            request.metadata_mut().insert("authorization", header);
        }

        let response = match client_archive.pull(request).await {
            Ok(response) => response,

            Err(status) => {
                if status.code() == Code::NotFound {
                    bail!("artifact not found in registry");
                }

                bail!("registry pull error: {status:?}");
            }
        };

        let mut stream = response.into_inner();
        let mut stream_data = Vec::new();

        loop {
            match stream.message().await {
                Ok(Some(chunk)) => {
                    if !chunk.data.is_empty() {
                        stream_data.extend_from_slice(&chunk.data);
                    }
                }

                Ok(None) => break,

                Err(status) => {
                    if status.code() == Code::NotFound {
                        bail!("artifact not found in registry");
                    }

                    bail!("registry stream error: {status:?}");
                }
            }
        }

        if stream_data.is_empty() {
            bail!("registry returned empty archive for digest: {digest}");
        }

        // Write archive to local store

        publish_archive_bytes(&stream_data, &archive_path).await?;
    }

    // Unpack archive to output path

    info!("unpacking artifact: {digest}");

    publish_unpacked_output(&archive_path, output_path).await?;

    Ok(())
}

fn validate_binary_name(binary_name: &str) -> Result<()> {
    if binary_name.is_empty() {
        bail!(
            "binary name cannot be empty\n\
             \n\
             Provide a non-empty value for --bin, or omit it to use the artifact name."
        );
    }

    if binary_name.contains('/') || binary_name.contains('\\') {
        bail!(
            "invalid binary name '{}': must be a plain filename without path separators\n\
             \n\
             Use just the binary name, for example: --bin {}",
            binary_name,
            binary_name
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(binary_name),
        );
    }

    if binary_name.starts_with('.') {
        bail!("invalid binary name '{binary_name}': must not start with '.'",);
    }

    Ok(())
}

async fn resolve_binary(output_path: &Path, binary_name: &str) -> Result<PathBuf> {
    validate_binary_name(binary_name)?;

    let bin_dir = output_path.join("bin");
    let binary_path = bin_dir.join(binary_name);

    if !binary_path.exists() {
        let mut message = format!(
            "binary '{}' not found in artifact output\n\
             \n\
             Expected binary at: {}",
            binary_name,
            binary_path.display(),
        );

        // List available binaries if the bin/ directory exists
        if bin_dir.is_dir() {
            let mut available: Vec<String> = Vec::new();

            if let Ok(mut entries) = tokio::fs::read_dir(&bin_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(ft) = entry.file_type().await {
                        if ft.is_file() {
                            available.push(entry.file_name().to_string_lossy().to_string());
                        }
                    }
                }
            }

            available.sort();

            if available.is_empty() {
                message.push_str("\n\nThe bin/ directory exists but contains no files.");
            } else {
                message.push_str("\n\nAvailable binaries:");
                for name in &available {
                    write!(message, "\n\t{name}")?;
                }
                message.push_str("\n\nUse --bin <name> to select a different binary.");
            }
        } else {
            message.push_str("\n\nThe artifact output has no bin/ directory.");
        }

        bail!("{message}");
    }

    let metadata = tokio::fs::metadata(&binary_path)
        .await
        .with_context(|| format!("failed to read metadata for: {}", binary_path.display()))?;

    if metadata.permissions().mode() & 0o111 == 0 {
        bail!(
            "binary is not executable: {}\n\
             \n\
             The file exists but does not have execute permissions.",
            binary_path.display(),
        );
    }

    Ok(binary_path)
}

/// Resolves an artifact alias to its local alias file, checking the registry
/// and caching the result locally when the alias file does not exist yet.
/// Returns the resolved digest.
async fn resolve_alias_digest(
    alias: &str,
    alias_parsed: &vorpal_sdk::context::ArtifactAlias,
    system: ArtifactSystem,
    registry: &str,
) -> Result<String> {
    let alias_path = get_artifact_alias_path(
        &alias_parsed.name,
        &alias_parsed.namespace,
        system,
        &alias_parsed.tag,
    )?;

    if !alias_path.exists() {
        info!("alias not found locally, checking registry: {registry}");

        match get_alias_from_registry(
            registry,
            &alias_parsed.name,
            &alias_parsed.namespace,
            system,
            &alias_parsed.tag,
        )
        .await
        {
            Ok(digest) => {
                info!("alias resolved from registry: digest={digest}");

                if let Some(parent) = alias_path.parent() {
                    create_dir_all(parent).await.with_context(|| {
                        format!("failed to create alias directory: {}", parent.display())
                    })?;
                }

                // This writer publishes the alias by rename, so it never
                // leaves a truncated digest behind. That is a property of
                // this producer only: the local registry backend
                // (`start/registry/artifact/local.rs`) still writes the same
                // alias path in place, so a reader can still observe a
                // partial digest written by that one.
                let alias_staging_path = staging_path_for(&alias_path);

                let staged: Result<()> = async {
                    write(&alias_staging_path, digest.as_bytes())
                        .await
                        .with_context(|| {
                            format!(
                                "failed to write alias file: {}",
                                alias_staging_path.display()
                            )
                        })?;

                    publish_atomically(&alias_staging_path, &alias_path)
                        .await
                        .map(|_| ())
                }
                .await;

                if staged.is_err() {
                    discard_staging(&alias_staging_path).await;
                }

                staged?;
            }

            Err(err) => {
                debug!("registry alias lookup failed: {err}");

                bail!(
                    "artifact alias not found: {}\n\
                     \n\
                     The alias file does not exist at: {}\n\
                     \n\
                     The alias could not be resolved from the registry:\n\
                     \n\
                     \t{err}\n\
                     \n\
                     Have you built this artifact? Try:\n\
                     \n\
                     \tvorpal build {}",
                    alias,
                    alias_path.display(),
                    alias_parsed.name,
                );
            }
        }
    }

    read_alias_digest(&alias_path, &alias_parsed.name).await
}

pub async fn run(alias: &str, args: &[String], bin: Option<&str>, registry: &str) -> Result<()> {
    let alias_parsed = parse_artifact_alias(alias)?;

    parse_artifact_namespace(&alias_parsed.namespace)?;

    let system = get_system_default()?;

    debug!(
        "run: name={}, namespace={}, system={}, tag={}",
        alias_parsed.name,
        alias_parsed.namespace,
        system.as_str_name(),
        alias_parsed.tag,
    );

    // Step 1: Resolve artifact alias to digest

    let artifact_digest = resolve_alias_digest(alias, &alias_parsed, system, registry).await?;

    debug!("run: resolved digest={artifact_digest}");

    // Step 2: Locate the binary

    let output_path = get_artifact_output_path(&artifact_digest, &alias_parsed.namespace);

    if !output_path.exists() {
        info!("artifact output not found locally, attempting pull from registry");

        let pulled = pull_artifact_from_registry(
            registry,
            &artifact_digest,
            &alias_parsed.namespace,
            &output_path,
        )
        .await;

        match pulled {
            Ok(()) => {
                info!("artifact pulled from registry successfully");
            }

            Err(err) => {
                debug!("registry pull failed: {err}");

                bail!(
                    "artifact output not found for digest: {artifact_digest}\n\
                     \n\
                     The output directory does not exist at: {}\n\
                     \n\
                     The artifact could not be pulled from the registry:\n\
                     \n\
                     \t{err}\n\
                     \n\
                     The artifact may need to be rebuilt:\n\
                     \n\
                     \tvorpal build {}",
                    output_path.display(),
                    alias_parsed.name,
                );
            }
        }
    }

    let binary_name = bin.unwrap_or(&alias_parsed.name);
    let binary_path = resolve_binary(&output_path, binary_name).await?;

    // Step 3: Execute the binary, replacing this process

    let err = Command::new(&binary_path).args(args).exec();

    // exec() only returns on error
    bail!("failed to execute {}: {}", binary_path.display(), err,);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use tempfile::TempDir;

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

    // Builds a real `.tar.zst` at `archive_path` holding `files` as regular
    // entries and `dirs` as directory entries. `compress_zstd` cannot serve
    // here: it stages through the real store root.
    async fn write_tar_zst(archive_path: &Path, files: &[(&str, &str)], dirs: &[&str]) {
        use tokio::io::AsyncWriteExt;

        let file = tokio::fs::File::create(archive_path).await.unwrap();
        let encoder = async_compression::tokio::write::ZstdEncoder::new(file);
        let mut builder = tokio_tar::Builder::new(encoder);

        for name in dirs {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Directory);
            header.set_mode(0o755);
            header.set_size(0);

            builder
                .append_data(&mut header, name, tokio::io::empty())
                .await
                .unwrap();
        }

        for (name, contents) in files {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Regular);
            header.set_mode(0o644);
            header.set_size(contents.len() as u64);

            builder
                .append_data(&mut header, name, contents.as_bytes())
                .await
                .unwrap();
        }

        builder.finish().await.unwrap();

        let mut encoder = builder.into_inner().await.unwrap();

        encoder.shutdown().await.unwrap();
    }

    // C8, fail closed before the rename: an archive that unpacks to no
    // regular file — no entries at all, or directory entries only — must
    // never be published, because the `exists()` check `run` performs before
    // falling back to a pull would read that empty tree as a finished
    // artifact and then fail looking for a binary in it. The refused archive
    // is retired with it, so the next run re-pulls.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_an_archive_that_unpacked_no_files() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(&archive_path, &[], &["bin"]).await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("unpacked no files"),
            "a file-less archive was not refused: {err}"
        );
        assert!(
            !output_path.exists(),
            "a file-less archive was published as a finished artifact"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "refusing the archive left its staging directory under the store"
        );
        assert!(
            !archive_path.exists(),
            "the refused archive stayed cached, so every later run skips the pull and fails again"
        );
    }

    // C8, the other half: a publish that fails for any reason other than a
    // lost race must surface as an error rather than collapse to `Ok`, and
    // must leave no staging behind. A regular file at the output path makes
    // the publishing rename fail `ENOTDIR`, which is a genuine I/O failure.
    #[tokio::test]
    async fn publish_unpacked_output_reports_a_failed_publish_and_leaves_no_staging() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(&archive_path, &[("bin", "binary-content")], &[]).await;

        let output_path = store_path.join("abc123");

        std::fs::write(&output_path, b"not a directory").unwrap();

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("failed to publish"),
            "a genuine publish failure did not surface as an error: {err}"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "a failed publish left its staging directory under the store"
        );
    }

    // AC3, archive half, pinned at `run`'s own call site: a pulled archive
    // is staged elsewhere and published onto its shared path, so a file
    // already there is replaced whole rather than truncated in place under
    // a reader's open handle.
    #[tokio::test]
    async fn publish_archive_bytes_replaces_the_real_path_instead_of_writing_into_it() {
        let root = TempDir::new().unwrap();
        let archive_dir = root.path().join("archives");
        let archive_path = archive_dir.join("abc123.tar.zst");

        std::fs::create_dir_all(&archive_dir).unwrap();
        std::fs::write(&archive_path, b"first-bytes").unwrap();

        publish_archive_bytes(b"second-bytes", &archive_path)
            .await
            .unwrap();

        assert_eq!(std::fs::read(&archive_path).unwrap(), b"second-bytes");
        assert_eq!(
            dir_entry_names(&archive_dir),
            BTreeSet::from(["abc123.tar.zst".to_string()]),
            "a staging file was left under the store directory"
        );
    }

    // A digest names a store path, so a value that is not a bare sha256 hex
    // string lets whoever supplied it choose where `run` reads and executes
    // from. Both traversal and absolute forms must be refused before the
    // digest reaches `get_artifact_output_path`.
    #[tokio::test]
    async fn read_alias_digest_refuses_a_digest_that_is_not_a_bare_hex_string() {
        let root = TempDir::new().unwrap();

        for hostile in [
            "../../../../../../Users/u/Library/LaunchAgents",
            "/Users/u/.ssh",
            "ABC123",
            "abc123",
        ] {
            let alias_path = root.path().join("alias");

            std::fs::write(&alias_path, hostile).unwrap();

            let err = read_alias_digest(&alias_path, "example").await.unwrap_err();

            assert!(
                err.to_string().contains("invalid artifact digest"),
                "accepted a digest that does not name a store path: {hostile}"
            );
        }
    }

    #[tokio::test]
    async fn read_alias_digest_accepts_a_sha256_digest() {
        let root = TempDir::new().unwrap();
        let alias_path = root.path().join("alias");
        let digest = "a".repeat(64);

        std::fs::write(&alias_path, format!("{digest}\n")).unwrap();

        assert_eq!(
            read_alias_digest(&alias_path, "example").await.unwrap(),
            digest
        );
    }

    // AC1 at `run`'s own call site: publish_unpacked_output must never
    // unpack into the real output path. An artifact already published there
    // survives a pull whose archive turns out to be garbage, byte for byte
    // — an in-place unpack (the pre-fix `create_dir_all` + `unpack_zstd`
    // directly onto the real path) would create into that path and then
    // delete it while cleaning up.
    #[tokio::test]
    async fn publish_unpacked_output_leaves_an_already_published_output_path_untouched() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");
        let output_path = store_path.join("abc123");

        std::fs::create_dir_all(&output_path).unwrap();
        write_files(&output_path, &["published.txt"], "published-content");

        let archive_path = root.path().join("abc123.tar.zst");

        std::fs::write(&archive_path, "not a zstd archive").unwrap();

        publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert_eq!(
            dir_entry_names(&output_path),
            BTreeSet::from(["published.txt".to_string()]),
            "a failed unpack disturbed an already published output path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::from(["abc123".to_string()]),
            "a failed unpack left its staging directory under the store"
        );
    }

    // AC2: an unpack that fails must leave nothing at the real output path,
    // so the `exists()` check `run` performs before falling back to a pull
    // can never mistake wreckage for a finished artifact.
    #[tokio::test]
    async fn publish_unpacked_output_leaves_the_real_output_path_absent_on_failure() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        std::fs::write(&archive_path, "not a zstd archive").unwrap();

        let output_path = store_path.join("abc123");

        publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            !output_path.exists(),
            "a failed unpack left debris at the real output path"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "a failed unpack left its staging directory under the store"
        );
    }

    // C1: `parse_artifact_alias`'s own component check allows `.`, so `..`
    // passes it and must be caught here instead, before it is joined into a
    // store path.
    #[test]
    fn parse_artifact_namespace_refuses_dot_and_dot_dot() {
        for hostile in [".", ".."] {
            let err = parse_artifact_namespace(hostile).unwrap_err();

            assert!(
                err.to_string().contains("invalid artifact namespace"),
                "accepted a namespace that traverses the store root: {hostile:?}"
            );
        }
    }

    #[test]
    fn parse_artifact_namespace_accepts_an_ordinary_namespace() {
        parse_artifact_namespace("library").unwrap();
        parse_artifact_namespace("my-namespace.v2").unwrap();
    }

    // C5, positive control: an archive whose regular files hold ordinary
    // content must still publish - the scan below must not turn into a
    // content check that rejects anything unexpected.
    #[tokio::test]
    async fn publish_unpacked_output_publishes_an_archive_with_ordinary_content() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(&archive_path, &[("bin", "ordinary binary content")], &[]).await;

        let output_path = store_path.join("abc123");

        publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap();

        assert_eq!(
            dir_entry_names(&output_path),
            BTreeSet::from(["bin".to_string()]),
        );
    }

    // C5 / AC-8: a registry-supplied archive whose only regular file embeds
    // a `.tmp-` staging-path reference must be refused rather than
    // published, so a local user cannot pre-create the directory a
    // concurrent producer's staging traffic will pass through.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_an_archive_embedding_a_staging_path_reference() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst(
            &archive_path,
            &[(
                "bin",
                "some content referencing /var/lib/vorpal/store/artifact/output/library/.tmp-deadbeef/x",
            )],
            &[],
        )
        .await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "an archive embedding a staging-path reference was not refused: {err}"
        );
        assert!(
            !output_path.exists(),
            "an archive embedding a staging-path reference was published"
        );
        assert_eq!(
            dir_entry_names(&store_path),
            BTreeSet::new(),
            "refusing the archive left its staging directory under the store"
        );
        assert!(
            !archive_path.exists(),
            "the refused archive stayed cached, so every later run skips the pull and fails again"
        );
    }
}
