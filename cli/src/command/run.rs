use crate::command::store::{
    archives::unpack_zstd,
    paths::{
        discard_staging, get_artifact_alias_path, get_artifact_archive_path,
        get_artifact_output_path, publish_atomically, set_timestamps, staging_path_for,
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
use walkdir::WalkDir;

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
/// `is_valid_component`) allows `.`, so `..` passes it and resolves one level
/// up when joined into a store path. It allows that for every field it
/// parses, and this process joins all three - namespace, name and tag - into
/// `get_artifact_alias_path`, so each one is checked here.
///
/// Returns the component rather than `()` so the checked value is what
/// callers go on to use: a call site that drops the parse stops compiling
/// instead of quietly joining the unchecked string into a store path.
/// `build.rs` defines the same function, with the same predicate, for the
/// namespace it takes from `Vorpal.toml`.
fn parse_store_path_component(value: &str, field: &str) -> Result<String> {
    let staging_prefix = staging_path_prefix();

    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.starts_with(&staging_prefix)
    {
        bail!(
            "invalid artifact {field} {:?}: must be non-empty, contain no path separator, \
             not be '.' or '..', and not start with the reserved staging prefix {:?}",
            value,
            staging_prefix,
        );
    }

    Ok(value.to_string())
}

/// Text length of the hyphenated UUID `staging_path_for` appends to every
/// staging path's name.
const STAGING_UUID_LENGTH: usize = 36;

const STAGING_SCAN_CHUNK_SIZE: usize = 8192;

/// The prefix `staging_path_for` (`store/paths.rs`) names every staging path
/// with, read back out of that function rather than written out here, so
/// renaming the prefix there cannot leave this file compiling and matching
/// nothing.
fn staging_path_prefix() -> String {
    let sample = staging_path_for(Path::new("/entry"));

    let name = sample
        .file_name()
        .expect("a staging path is a named sibling of the entry it stages")
        .to_string_lossy()
        .to_string();

    name[..name.len() - STAGING_UUID_LENGTH].to_string()
}

/// Whether `window` opens with a whole staging path component: a `/`
/// separator, the staging prefix, and a hyphenated-UUID-shaped name.
///
/// The shape is the anchor. A bare occurrence of the prefix is ordinary
/// content — a version string like `sequal.tmp-1.0.8`, a regular-expression
/// source, the format string inside vorpal's own binary — and matching it
/// would refuse legitimate artifacts, while what this control is looking for
/// is a reference to a store path some other producer stages through.
fn is_staging_path_reference(window: &[u8], needle: &[u8]) -> bool {
    if !window.starts_with(needle) {
        return false;
    }

    let name = &window[needle.len()..];

    name.len() >= STAGING_UUID_LENGTH
        && name[..STAGING_UUID_LENGTH]
            .iter()
            .enumerate()
            .all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => *byte == b'-',
                _ => byte.is_ascii_hexdigit(),
            })
}

/// Reports the first staged entry whose contents or symlink target name a
/// staging path — `<store path>/.tmp-<uuid>`.
///
/// This producer's corpus is archive content from whoever the registry
/// forwards, not content this process staged itself, so an archive that
/// embeds such a path hands whoever crafted it a path a concurrent build's
/// own staging traffic will pass through once this producer's rename retires
/// the directory the archive named. Unlike the worker's own
/// embedded-reference scan (which matches the one staging name a single build
/// used), this one matches any staging name.
///
/// Files are read in overlapping chunks so a reference straddling a read
/// boundary is still found and an artifact larger than memory is still
/// scannable.
///
/// This finds a literal occurrence, so like the worker's own scan it binds a
/// naive producer only: an archive that encodes, compresses or splits the
/// path defeats it. It is a guard against an artifact recording a staging
/// path — accidentally, or to squat one — not a control that makes a hostile
/// archive safe to publish.
async fn find_staging_path_reference(staged_files: &[PathBuf]) -> Result<Option<PathBuf>> {
    let needle = format!("/{}", staging_path_prefix()).into_bytes();
    let window_size = needle.len() + STAGING_UUID_LENGTH;
    let overlap = window_size - 1;

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
                .windows(window_size)
                .any(|window| is_staging_path_reference(window, &needle))
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

            if buf[..filled]
                .windows(window_size)
                .any(|window| is_staging_path_reference(window, &needle))
            {
                return Ok(Some(path.clone()));
            }

            carried = overlap.min(filled);
            buf.copy_within(filled - carried..filled, 0);
        }
    }

    Ok(None)
}

/// Every entry under a staged tree, with no exclusions and no entry dropped:
/// this is the corpus the scan above judges, so a subtree missing from it is
/// a subtree that publishes unread. `get_file_paths` cannot serve — it drops
/// `.git` unconditionally and swallows any entry the walk cannot read.
fn staged_entry_paths(staging_path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for entry in WalkDir::new(staging_path) {
        let entry = entry.map_err(|err| {
            anyhow!(
                "failed to walk staged tree {}: {err}",
                staging_path.display()
            )
        })?;

        paths.push(entry.path().to_path_buf());
    }

    paths.sort();

    Ok(paths)
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

        let staged_files = staged_entry_paths(&staging_path)?;

        // `staged_entry_paths` walks from the staging root down and reports
        // every entry, so its result is never empty for a directory that exists and
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

        let scanned = match find_staging_path_reference(&staged_files).await {
            Ok(scanned) => scanned,

            // The scan failed on this host - an unreadable staged file, a
            // full disk - rather than judging the archive. Retire the archive
            // so the next invocation re-pulls and scans it, instead of
            // short-circuiting forever on a cached archive that was never
            // judged.
            Err(err) => {
                retire_atomically(archive_path).await?;

                return Err(err);
            }
        };

        if let Some(offender) = scanned {
            // The archive is kept, unlike the emptiness bail above: this
            // verdict is a function of the archive's bytes alone, so a
            // re-pull of the same digest reproduces it exactly. Retiring it
            // would turn a permanent refusal into a download on every
            // invocation. The next one refuses again, from cache, with the
            // same message.
            let offender = offender
                .strip_prefix(&staging_path)
                .unwrap_or(&offender)
                .display()
                .to_string()
                .escape_default()
                .to_string();

            bail!(
                "archive embeds a staging-path reference in \"{offender}\", which does not \
                 survive publishing"
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

pub async fn run(alias: &str, args: &[String], bin: Option<&str>, registry: &str) -> Result<()> {
    let alias_parsed = parse_artifact_alias(alias)?;

    let artifact_name = parse_store_path_component(&alias_parsed.name, "name")?;
    let artifact_namespace = parse_store_path_component(&alias_parsed.namespace, "namespace")?;
    let artifact_tag = parse_store_path_component(&alias_parsed.tag, "tag")?;

    let system = get_system_default()?;

    debug!(
        "run: name={}, namespace={}, system={}, tag={}",
        artifact_name,
        artifact_namespace,
        system.as_str_name(),
        artifact_tag,
    );

    // Step 1: Resolve artifact alias to digest

    let alias_path =
        get_artifact_alias_path(&artifact_name, &artifact_namespace, system, &artifact_tag)?;

    if !alias_path.exists() {
        info!("alias not found locally, checking registry: {registry}");

        match get_alias_from_registry(
            registry,
            &artifact_name,
            &artifact_namespace,
            system,
            &artifact_tag,
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
                // leaves a truncated digest behind.
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
                    artifact_name,
                );
            }
        }
    }

    let artifact_digest = read_alias_digest(&alias_path, &artifact_name).await?;

    debug!("run: resolved digest={artifact_digest}");

    // Step 2: Locate the binary

    let output_path = get_artifact_output_path(&artifact_digest, &artifact_namespace);

    if !output_path.exists() {
        info!("artifact output not found locally, attempting pull from registry");

        let pulled = pull_artifact_from_registry(
            registry,
            &artifact_digest,
            &artifact_namespace,
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
                    artifact_name,
                );
            }
        }
    }

    let binary_name = bin.unwrap_or(&artifact_name);
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

    // A staging path as an artifact would embed it: an absolute store path
    // whose last component is what `staging_path_for` names one.
    const EMBEDDED_STAGING_PATH: &str = concat!(
        "/var/lib/vorpal/store/artifact/output/library/",
        ".tmp-0190dead-beef-7000-8000-0123456789ab/bin/x"
    );

    // Builds a real `.tar.zst` at `archive_path` holding `files` as regular
    // entries (name, contents, mode), `dirs` as directory entries and `links`
    // as symlink entries (name, target). `compress_zstd` cannot serve here:
    // it stages through the real store root.
    async fn write_tar_zst_entries(
        archive_path: &Path,
        files: &[(&str, &str, u32)],
        dirs: &[&str],
        links: &[(&str, &str)],
    ) {
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

        for (name, contents, mode) in files {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Regular);
            header.set_mode(*mode);
            header.set_size(contents.len() as u64);

            builder
                .append_data(&mut header, name, contents.as_bytes())
                .await
                .unwrap();
        }

        for (name, target) in links {
            let mut header = tokio_tar::Header::new_gnu();

            header.set_entry_type(tokio_tar::EntryType::Symlink);
            header.set_mode(0o777);
            header.set_size(0);
            header.set_link_name(target).unwrap();

            builder
                .append_data(&mut header, name, tokio::io::empty())
                .await
                .unwrap();
        }

        builder.finish().await.unwrap();

        let mut encoder = builder.into_inner().await.unwrap();

        encoder.shutdown().await.unwrap();
    }

    async fn write_tar_zst(archive_path: &Path, files: &[(&str, &str)], dirs: &[&str]) {
        let files: Vec<(&str, &str, u32)> = files
            .iter()
            .map(|(name, contents)| (*name, *contents, 0o644))
            .collect();

        write_tar_zst_entries(archive_path, &files, dirs, &[]).await;
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
            // Right length, wrong alphabet: the only fixtures that reach the
            // character-class half of the check rather than failing on length.
            &"g".repeat(64),
            &"A".repeat(64),
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
    // store path. Separators are checked here too rather than left to that
    // upstream parse, so this file's own guarantee does not depend on a
    // caller having gone through it.
    #[test]
    fn parse_store_path_component_refuses_traversal_and_separators() {
        for hostile in ["..", ".", "", "../escape", "a/b", "a\\b"] {
            for field in ["name", "namespace", "tag"] {
                let err = parse_store_path_component(hostile, field).unwrap_err();

                assert!(
                    err.to_string()
                        .contains(&format!("invalid artifact {field}")),
                    "accepted a {field} that traverses the store root: {hostile:?}"
                );
            }
        }
    }

    // A component that opens with the staging prefix names a directory
    // indistinguishable from a staging path, so a later sweeper of `.tmp-*`
    // would take a published alias directory for abandoned staging.
    #[test]
    fn parse_store_path_component_refuses_the_reserved_staging_prefix() {
        let reserved = format!("{}0190deadbeef", staging_path_prefix());

        let err = parse_store_path_component(&reserved, "namespace").unwrap_err();

        assert!(
            err.to_string().contains("invalid artifact namespace"),
            "accepted a namespace that names a staging path: {err}"
        );
    }

    #[test]
    fn parse_store_path_component_accepts_an_ordinary_component() {
        assert_eq!(
            parse_store_path_component("library", "namespace").unwrap(),
            "library"
        );
        assert_eq!(
            parse_store_path_component("my-namespace.v2", "namespace").unwrap(),
            "my-namespace.v2"
        );
    }

    // The scan's needle is derived from `staging_path_for` rather than
    // spelled out, so this pins the two together: renaming the prefix there
    // fails here rather than leaving the scan matching nothing.
    #[test]
    fn staging_path_prefix_is_the_name_staging_path_for_produces() {
        let staging_path = staging_path_for(Path::new("/store/entry"));
        let name = staging_path.file_name().unwrap().to_string_lossy();

        assert!(
            name.starts_with(&staging_path_prefix()),
            "the scan's needle no longer matches what staging_path_for names: {name}"
        );
        assert_eq!(
            name.len(),
            staging_path_prefix().len() + STAGING_UUID_LENGTH,
            "the staging name's uuid is no longer the length the scan assumes: {name}"
        );
    }

    // C5, positive control: the scan matches a staging path, not the prefix
    // that names one. Every string here contains that prefix and none of
    // them names a store path a producer stages through — a version suffix,
    // a plain word, a regular-expression source, and the real store path an
    // artifact is expected to reference — so all of them must publish.
    // vorpal's own `bin/vorpal` carries the fourth of these (the format
    // string in `staging_path_for`), and a scan matching the bare prefix
    // refuses it.
    #[tokio::test]
    async fn publish_unpacked_output_publishes_content_that_merely_contains_the_staging_prefix() {
        for content in [
            "ordinary binary content",
            "sequal.tmp-1.0.8",
            "tmp-0190dead-beef-7000-8000-0123456789ab",
            ".tmpfile",
            "/^\\.tmp-/ and \".tmp-{}\"",
            "/var/lib/vorpal/store/artifact/output/library/abc123/bin/x",
        ] {
            let root = TempDir::new().unwrap();
            let store_path = root.path().join("output");

            std::fs::create_dir_all(&store_path).unwrap();

            let archive_path = root.path().join("abc123.tar.zst");

            write_tar_zst(&archive_path, &[("bin", content)], &[]).await;

            let output_path = store_path.join("abc123");

            publish_unpacked_output(&archive_path, &output_path)
                .await
                .unwrap_or_else(|err| panic!("refused ordinary content {content:?}: {err}"));

            assert_eq!(
                dir_entry_names(&output_path),
                BTreeSet::from(["bin".to_string()]),
            );
        }
    }

    // C5 / AC-8: a registry-supplied archive whose only regular file embeds
    // a staging path must be refused rather than published, so a local user
    // cannot pre-create the directory a concurrent producer's staging
    // traffic will pass through.
    //
    // The archive is kept: the verdict is a function of its bytes, so a
    // re-pull of the same digest reproduces it exactly and retiring it would
    // buy a download per invocation and nothing else.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_an_archive_embedding_a_staging_path_reference() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");
        let content = format!("some content referencing {EMBEDDED_STAGING_PATH}");

        write_tar_zst(&archive_path, &[("bin", &content)], &[]).await;

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
            archive_path.exists(),
            "a refusal the archive's own bytes determine retired it, so the next run re-pulls the \
             same bytes to reach the same verdict"
        );
    }

    // The scan reads a chunk plus one window of overlap at a time, and a
    // hostile archive only has to place its reference across that boundary.
    // This one starts two bytes before it, so it is found only by carrying
    // the tail of one read into the next.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_a_reference_that_straddles_a_scan_chunk() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");
        let window = 1 + staging_path_prefix().len() + STAGING_UUID_LENGTH;
        let read_size = STAGING_SCAN_CHUNK_SIZE + window - 1;
        let offset = EMBEDDED_STAGING_PATH
            .find(&format!("/{}", staging_path_prefix()))
            .unwrap();
        let content = format!(
            "{}{EMBEDDED_STAGING_PATH}",
            "f".repeat(read_size - 2 - offset)
        );

        write_tar_zst(&archive_path, &[("bin", &content)], &[]).await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "a reference split across two scan chunks was not found: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
    }

    // A symlink target is a path an artifact resolves at run time, so a link
    // into a staging path is the same squat as the reference in a file's
    // bytes. The regular file is there to carry the tree past the emptiness
    // bail.
    #[tokio::test]
    async fn publish_unpacked_output_refuses_a_symlink_into_a_staging_path() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst_entries(
            &archive_path,
            &[("bin", "ordinary binary content", 0o644)],
            &[],
            &[("lib", EMBEDDED_STAGING_PATH)],
        )
        .await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "a symlink into a staging path was not refused: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
    }

    // The scan's corpus is the whole staged tree. A dotted directory is
    // ordinary artifact content and is published like any other, so leaving
    // it out of the walk hands an archive a free hiding place.
    #[tokio::test]
    async fn publish_unpacked_output_scans_dotted_directories_too() {
        let root = TempDir::new().unwrap();
        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");
        let content = format!("some content referencing {EMBEDDED_STAGING_PATH}");

        write_tar_zst(
            &archive_path,
            &[
                ("bin", "ordinary binary content"),
                (".git/config", &content),
            ],
            &[".git"],
        )
        .await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("embeds a staging-path reference"),
            "a reference under a dotted directory escaped the scan: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
    }

    // A scan that cannot read a staged file has judged nothing, so the
    // archive must not stay cached: the failure is this host's, and a later
    // invocation that finds the archive already there would skip the pull
    // and never scan those bytes at all.
    #[tokio::test]
    async fn publish_unpacked_output_retires_the_archive_when_the_scan_cannot_read_a_file() {
        let root = TempDir::new().unwrap();
        let probe_path = root.path().join("probe");

        std::fs::write(&probe_path, b"probe").unwrap();
        std::fs::set_permissions(&probe_path, std::fs::Permissions::from_mode(0o000)).unwrap();

        if std::fs::File::open(&probe_path).is_ok() {
            // Running with privileges that ignore file modes, so an
            // unreadable staged file cannot be arranged here.
            return;
        }

        let store_path = root.path().join("output");

        std::fs::create_dir_all(&store_path).unwrap();

        let archive_path = root.path().join("abc123.tar.zst");

        write_tar_zst_entries(&archive_path, &[("bin", "content", 0o000)], &[], &[]).await;

        let output_path = store_path.join("abc123");

        let err = publish_unpacked_output(&archive_path, &output_path)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("failed to open staged file"),
            "an unreadable staged file did not surface as a scan failure: {err}"
        );
        assert!(!output_path.exists(), "the archive was published anyway");
        assert!(
            !archive_path.exists(),
            "an archive the scan never judged stayed cached, so every later run skips the pull"
        );
    }
}
