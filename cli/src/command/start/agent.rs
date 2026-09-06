use crate::command::{
    lock::{artifact_system_to_platform, load_lock, save_lock, LockSource, Lockfile},
    start::{auth, resolve_registry, ResolvedRegistry},
    store::{
        archives::{compress_zstd, unpack_zip},
        hashes::get_source_digest,
        notary,
        paths::{
            copy_files, get_file_paths, get_key_service_key_path, get_key_service_public_path,
            set_timestamps,
        },
        temps::{create_sandbox_dir, create_sandbox_file},
        DUPLEX_BUF_SIZE,
    },
};
use anyhow::{anyhow, bail, Result};
use async_compression::tokio::bufread::{BzDecoder, GzipDecoder};
use sha256::digest;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::{
    fs::{remove_dir_all, remove_file, write, File},
    io::{AsyncReadExt, BufReader},
    sync::{
        mpsc::{channel, Sender},
        Mutex,
    },
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tar::Archive;
use tokio_util::io::SyncIoBridge;
use tonic::{Code, Request, Response, Status};
use tracing::{info, warn};
use url::Url;
use vorpal_sdk::{
    api::{
        agent::{
            agent_service_server::AgentService, PrepareArtifactRequest, PrepareArtifactResponse,
        },
        archive::{
            archive_service_client::ArchiveServiceClient, ArchivePullRequest, ArchivePushRequest,
        },
        artifact::{Artifact, ArtifactSource, ArtifactStep, ArtifactStepSecret},
    },
    context::{build_channel, client_auth_header},
};

#[derive(Debug, PartialEq)]
enum ArtifactSourceType {
    Unknown,
    Local,
    Git,
    Http,
}

/// Classify a source path: local if it exists on disk (workspace-trust domain,
/// re-read each build), otherwise git/http by scheme, else unknown.
fn classify_source_type(path: &str) -> ArtifactSourceType {
    if Path::new(path).exists() {
        ArtifactSourceType::Local
    } else if path.starts_with("git") {
        ArtifactSourceType::Git
    } else if path.starts_with("http://") || path.starts_with("https://") {
        ArtifactSourceType::Http
    } else {
        ArtifactSourceType::Unknown
    }
}

/// A remote (git/http) source with neither an inline digest nor a matching
/// (name, platform) lock entry is unpinned (TOFU). Minting that first pin
/// requires an explicit `--unlock`, symmetric with the change-guard that
/// already gates CHANGING an existing pin. Local sources are exempt.
fn requires_unlock_to_pin(
    source_type: &ArtifactSourceType,
    has_inline_digest: bool,
    has_lock_entry: bool,
) -> bool {
    let is_remote = matches!(
        source_type,
        ArtifactSourceType::Git | ArtifactSourceType::Http
    );

    is_remote && !has_inline_digest && !has_lock_entry
}

const DEFAULT_CHUNKS_SIZE: usize = 8192; // streaming chunk size

/// Unpacks a downloaded HTTP source `response_bytes` into `source_sandbox` according to
/// its detected `mime_type`. Recognized archive formats are extracted; unrecognized
/// mime-types are rejected.
async fn unpack_by_mime_type(
    mime_type: &str,
    response_bytes: &[u8],
    response_bytes_owned: &bytes::Bytes,
    artifact_source: &ArtifactSource,
    path: &Url,
    source_sandbox: &Path,
) -> Result<()> {
    match mime_type {
        "application/x-executable" | "application/x-mach-binary" => {
            let file_name = path
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .and_then(|name| if name.is_empty() { None } else { Some(name) })
                .unwrap_or(&artifact_source.name);

            let file_path = source_sandbox.join(file_name);

            write(&file_path, response_bytes)
                .await
                .map_err(|e| anyhow!(e))?;
        }

        "application/gzip" => {
            let decoder = GzipDecoder::new(response_bytes);
            let mut archive = Archive::new(decoder);

            archive
                .unpack(source_sandbox)
                .await
                .map_err(|e| anyhow!(e))?;
        }

        "application/x-bzip2" => {
            let decoder = BzDecoder::new(response_bytes);
            let mut archive = Archive::new(decoder);

            archive
                .unpack(source_sandbox)
                .await
                .map_err(|e| anyhow!(e))?;
        }

        "application/x-xz" => {
            let (async_reader, async_writer) = tokio::io::duplex(DUPLEX_BUF_SIZE);
            // response_bytes_owned is borrowed; spawn_blocking's future must own
            // its captures (cheap: Bytes::clone is a refcount bump).
            let compressed = response_bytes_owned.clone();
            let url = path.to_string();

            let decompress_fut = tokio::task::spawn_blocking(move || {
                let mut sync_writer = SyncIoBridge::new(async_writer);
                let input = std::io::Cursor::new(compressed.as_ref());
                let mut decoder = liblzma::read::XzDecoder::new(input);
                std::io::copy(&mut decoder, &mut sync_writer)
                    .map_err(|e| anyhow!("xz decompression failed for {url}: {e}"))?;
                Ok::<(), anyhow::Error>(())
            });

            let mut archive = Archive::new(async_reader);

            // Use join! (not try_join!) to always await the blocking task.
            let (decompress_result, unpack_result) = tokio::join!(
                async {
                    decompress_fut
                        .await
                        .map_err(|e| anyhow!("xz task join error: {e}"))?
                },
                async { archive.unpack(source_sandbox).await.map_err(|e| anyhow!(e)) },
            );

            unpack_result?;
            decompress_result?;
        }

        "application/zip" => {
            let archive_sandbox_path = create_sandbox_file(Some("zip")).await?;

            write(&archive_sandbox_path, response_bytes)
                .await
                .map_err(|e| anyhow!(e))?;

            unpack_zip(&archive_sandbox_path, source_sandbox).await?;

            remove_file(&archive_sandbox_path)
                .await
                .map_err(|e| anyhow!(e))?;
        }

        _ => {
            bail!(
                "'source.{}.path' unsupported mime-type detected: {:?}",
                artifact_source.name,
                artifact_source.path
            );
        }
    }

    Ok(())
}

/// Downloads an HTTP(S) artifact source into `source_sandbox`, detecting the payload's
/// mime-type and unpacking it if it's a recognized archive format, or writing it as a
/// plain file otherwise. Reports progress on `tx`.
async fn download_and_unpack_http_source(
    artifact_source: &ArtifactSource,
    source_sandbox: &Path,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<()> {
    // If a digest is provided, we'll later verify it matches the computed digest
    // from the downloaded content. If not provided, proceed and compute it.

    let path = Url::parse(&artifact_source.path).map_err(|e| anyhow!(e))?;

    if path.scheme() != "http" && path.scheme() != "https" {
        bail!("remote scheme not supported: {:?}", path.scheme());
    }

    let _ = tx
        .send(Ok(PrepareArtifactResponse {
            artifact: None,
            artifact_digest: None,
            artifact_output: Some(format!("download source: {path}")),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"));

    let response = reqwest::get(path.as_str()).await.map_err(|e| anyhow!(e))?;

    if !response.status().is_success() {
        bail!("URL not failed: {:?}", response.status());
    }

    let response_bytes_owned = response.bytes().await.map_err(|e| anyhow!(e))?;
    let response_bytes = response_bytes_owned.as_ref();
    let response_kind = infer::get(response_bytes);

    match response_kind {
        None => {
            warn!(
                "agent |> no mime-type detected for source: {}",
                artifact_source.name
            );

            let file_name = path
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .and_then(|name| if name.is_empty() { None } else { Some(name) })
                .unwrap_or(&artifact_source.name);

            let file_path = source_sandbox.join(file_name);

            write(&file_path, response_bytes)
                .await
                .map_err(|e| anyhow!(e))?;
        }

        Some(kind) => {
            info!(
                "agent |> detected mime-type: {} for source: {}",
                kind.mime_type(),
                artifact_source.name
            );

            let _ = tx
                .send(Ok(PrepareArtifactResponse {
                    artifact: None,
                    artifact_digest: None,
                    artifact_output: Some(format!("unpack source: {path}")),
                }))
                .await
                .map_err(|_| Status::internal("failed to send response"));

            unpack_by_mime_type(
                kind.mime_type(),
                response_bytes,
                &response_bytes_owned,
                artifact_source,
                &path,
                source_sandbox,
            )
            .await?;
        }
    }

    Ok(())
}

/// Packs `source_sandbox_files` into a zstd archive and streams it to the registry's
/// archive service under `source_digest`/`artifact_namespace`, then removes the local
/// archive file. Called only when the registry does not already have this source digest.
async fn pack_and_push_source<T>(
    client_archive: &mut ArchiveServiceClient<T>,
    source_sandbox: &std::path::PathBuf,
    source_sandbox_files: &[std::path::PathBuf],
    source_digest: &str,
    artifact_namespace: &str,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<()>
where
    T: tonic::client::GrpcService<tonic::body::Body>,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    T::ResponseBody: tonic::codegen::Body<Data = bytes::Bytes> + Send + 'static,
    <T::ResponseBody as tonic::codegen::Body>::Error:
        Into<Box<dyn std::error::Error + Send + Sync>> + Send,
{
    let source_sandbox_archive = create_sandbox_file(Some("tar.zst")).await?;

    let _ = tx
        .send(Ok(PrepareArtifactResponse {
            artifact: None,
            artifact_digest: None,
            artifact_output: Some(format!("pack source: {source_digest}")),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"));

    compress_zstd(
        source_sandbox,
        source_sandbox_files,
        &source_sandbox_archive,
    )
    .await?;

    let private_key_path = get_key_service_key_path();

    if !private_key_path.exists() {
        bail!("Private key not found: {}", private_key_path.display());
    }

    let _ = tx
        .send(Ok(PrepareArtifactResponse {
            artifact: None,
            artifact_digest: None,
            artifact_output: Some(format!("push source: {source_digest}")),
        }))
        .await
        .map_err(|_| Status::internal("failed to send response"));

    let (stream_tx, stream_rx) = tokio::sync::mpsc::channel(4);
    // source_sandbox_archive is a borrowed param still used below (remove_file)
    // after the spawned task, which must own its own copy of the path.
    let archive_path = source_sandbox_archive.clone();
    let digest_clone = source_digest.to_string();
    let namespace_clone = artifact_namespace.to_string();

    tokio::spawn(async move {
        let file = match File::open(&archive_path).await {
            Ok(f) => f,
            Err(e) => {
                warn!("agent |> failed to open archive: {}", e);
                return;
            }
        };
        let mut reader = BufReader::new(file);
        let mut buf = vec![0u8; DEFAULT_CHUNKS_SIZE];

        loop {
            let n = match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    warn!("agent |> failed to read archive chunk: {}", e);
                    return;
                }
            };

            // Each loop iteration sends an owned request; digest_clone/namespace_clone
            // are needed again on the next iteration.
            let request_push = ArchivePushRequest {
                data: buf[..n].to_vec(),
                digest: digest_clone.clone(),
                namespace: namespace_clone.clone(),
            };

            if stream_tx.send(request_push).await.is_err() {
                warn!("agent |> stream receiver dropped");
                return;
            }
        }
    });

    let request = Request::new(ReceiverStream::new(stream_rx));

    // Every prepared source now reaches this call, so a registry that
    // refuses the push must surface as an error rather than panicking the
    // task that serves the request.
    let pushed = client_archive
        .push(request)
        .await
        .map_err(|e| anyhow!("failed to push: {e}"));

    // The archive is a sandbox file this agent owns, and a failed push
    // leaves nothing worth keeping, so it is removed either way. A cleanup
    // failure must not displace the push failure that explains it, so the
    // push result is the one reported when both fail.
    if let Err(e) = remove_file(&source_sandbox_archive).await {
        warn!("agent |> failed to remove source archive: {}", e);

        pushed?;

        bail!("failed to remove source archive: {:?}", e);
    }

    pushed?;

    Ok(())
}

/// Materializes an artifact source's files into a fresh sandbox directory: downloads and
/// unpacks an HTTP source, or copies a local source's files, then returns the sandboxed
/// file list. Errors if no files result (git sources are rejected before this is called).
async fn materialize_source_files(
    source_type: &ArtifactSourceType,
    artifact_context: &str,
    artifact_source: &ArtifactSource,
    source_sandbox: &Path,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<Vec<std::path::PathBuf>> {
    if *source_type == ArtifactSourceType::Http {
        download_and_unpack_http_source(artifact_source, source_sandbox, tx).await?;
    }

    if *source_type == ArtifactSourceType::Local {
        let artifact_context = Path::new(artifact_context).to_path_buf();

        if !artifact_context.exists() {
            bail!("artifact not found in: {}", artifact_context.display());
        }

        // get_file_paths takes owned Vecs (shared with build.rs/run.rs callers
        // outside this crate's start/ scope); artifact_source is a shared borrow
        // reused for the second get_file_paths call below and afterward.
        let local_files = get_file_paths(
            &artifact_context,
            artifact_source.excludes.clone(),
            artifact_source.includes.clone(),
        )?;

        copy_files(&artifact_context, local_files, source_sandbox).await?;
    }

    let source_sandbox_files = get_file_paths(
        &source_sandbox.to_path_buf(),
        artifact_source.excludes.clone(),
        artifact_source.includes.clone(),
    )?;

    if source_sandbox_files.is_empty() {
        bail!(
            "Artifact 'source.{}.path' no files found: {:?}",
            artifact_source.name,
            artifact_source.path
        );
    }

    Ok(source_sandbox_files)
}

pub async fn build_source(
    artifact_context: String,
    artifact_namespace: String,
    artifact_source: &ArtifactSource,
    artifact_unlock: bool,
    registry: &ResolvedRegistry,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
) -> Result<String> {
    // Create authenticated archive client first
    let channel = build_channel(registry).await?;

    let client_auth_header = client_auth_header(registry)
        .await
        .map_err(|e| anyhow!("failed to get client auth header: {e}"))?;

    let mut client_archive =
        ArchiveServiceClient::with_interceptor(channel, move |mut req: Request<()>| {
            // The Fn interceptor closure may be invoked more than once per client;
            // each call needs its own owned header without consuming the capture.
            if let Some(header) = client_auth_header.clone() {
                req.metadata_mut().insert("authorization", header);
            }

            Ok(req)
        });

    let source_type = classify_source_type(&artifact_source.path);

    if source_type == ArtifactSourceType::Unknown {
        bail!(
            "'source.{}.path' unknown kind: {:?}",
            artifact_source.name,
            artifact_source.path
        );
    }

    if let Some(digest) = &artifact_source.digest {
        // digest and artifact_namespace are both used again below (return / push)
        let request = ArchivePullRequest {
            digest: digest.clone(),
            namespace: artifact_namespace.clone(),
        };

        match client_archive.check(request).await {
            Err(status) => {
                if status.code() != Code::NotFound {
                    bail!("registry check error: {status:?}");
                }
            }

            Ok(_) => {
                return Ok(digest.clone());
            }
        }
    }

    // 2. Build source

    if source_type == ArtifactSourceType::Git {
        bail!("'source.{}.path' git not supported", artifact_source.name);
    }

    let source_sandbox = create_sandbox_dir().await?;

    let source_sandbox_files = materialize_source_files(
        &source_type,
        &artifact_context,
        artifact_source,
        &source_sandbox,
        tx,
    )
    .await?;

    // 3. Sanitize files

    for sandbox_path in &source_sandbox_files {
        set_timestamps(sandbox_path).await?;
    }

    // 4. Digest files

    let source_digest = get_source_digest(&source_sandbox_files)?;

    if let Some(digest) = artifact_source.digest.as_ref() {
        if !artifact_unlock && source_digest != *digest {
            bail!(
                "'source.{}.digest' mismatch: {} != {}",
                artifact_source.name,
                source_digest,
                digest
            );
        }

        if source_digest == *digest {
            info!(
                "agent |> verified source: {} ({})",
                artifact_source.name, source_digest
            );
        }
    }

    // 5. Push source
    //
    // The push is unconditional, matching the worker's publication
    // (`start/worker.rs`, `publish_to_registry`). The only way to ask whether
    // the registry already holds this archive — `Archive/Check` — is served
    // from a TTL cache that neither a push nor a delete invalidates
    // (`start/registry.rs`), so a stale "yes" would skip the push and leave a
    // prepared artifact naming a source the registry cannot serve. Both
    // archive backends already return early for an archive they hold
    // (`start/registry/archive/local.rs` and `.../s3.rs`), so the repeated
    // push costs an upload, not a correctness hazard. An existence check may
    // buy back cost; it may not decide an invariant.

    pack_and_push_source(
        &mut client_archive,
        &source_sandbox,
        &source_sandbox_files,
        &source_digest,
        &artifact_namespace,
        tx,
    )
    .await?;

    remove_dir_all(&source_sandbox)
        .await
        .map_err(|e| anyhow!(e))?;

    Ok(source_digest)
}

/// The namespace a prepare was authorized for, plus every field of an artifact
/// source except `digest`. The namespace is the string the handler's write gate
/// checked, so a cached digest can only be served back to the namespace whose
/// authorization produced it.
#[derive(Debug, Eq, Hash, PartialEq)]
struct SourceCacheKey {
    artifact_namespace: String,
    excludes: Vec<String>,
    includes: Vec<String>,
    name: String,
    path: String,
    platform: String,
}

/// Combined cache of source digests resolved during this session.
#[derive(Debug, Default)]
struct SourceCacheState {
    /// Key: namespace + all source fields (except digest) + platform -> computed source digest
    by_key: HashMap<SourceCacheKey, String>,
    /// Key: (namespace, `source_url`, excludes, includes, platform) -> computed source digest (HTTP sources only)
    by_url: HashMap<(String, String, Vec<String>, Vec<String>, String), String>,
}

type SourceCache = Arc<Mutex<SourceCacheState>>;

/// The push-bearing step of a prepare, behind a trait so a test can observe
/// which namespaces a request actually pushes under. `build_source` reaches a
/// real registry and a writable store root, neither of which a unit test can
/// stand up here; the cache decision this seam sits behind is what the tests
/// need to pin.
#[tonic::async_trait]
trait SourceBuilder: Send + Sync {
    async fn build(
        &self,
        artifact_context: String,
        artifact_namespace: String,
        artifact_source: &ArtifactSource,
        artifact_unlock: bool,
        registry: &ResolvedRegistry,
        tx: &Sender<Result<PrepareArtifactResponse, Status>>,
    ) -> Result<String>;
}

struct RegistrySourceBuilder;

#[tonic::async_trait]
impl SourceBuilder for RegistrySourceBuilder {
    async fn build(
        &self,
        artifact_context: String,
        artifact_namespace: String,
        artifact_source: &ArtifactSource,
        artifact_unlock: bool,
        registry: &ResolvedRegistry,
        tx: &Sender<Result<PrepareArtifactResponse, Status>>,
    ) -> Result<String> {
        build_source(
            artifact_context,
            artifact_namespace,
            artifact_source,
            artifact_unlock,
            registry,
            tx,
        )
        .await
    }
}

/// Upserts `artifact_sources`'s last (just-resolved) entry into the on-disk lockfile at
/// `lock_path`, keyed by (name, platform), creating the lockfile if it does not exist.
/// Called only for HTTP sources.
async fn upsert_source_lockfile(
    lock_path: &Path,
    artifact_sources: &[ArtifactSource],
    artifact_source_digest: &str,
    target_platform: &str,
) -> Result<(), Status> {
    let mut lock = match load_lock(lock_path).await.unwrap_or(None) {
        Some(l) => l,
        None => Lockfile {
            lockfile: 1,
            sources: vec![],
        },
    };

    let Some(last) = artifact_sources.last() else {
        return Err(Status::internal("source list empty after push"));
    };
    let mut lockfile_modified = false;

    // Upsert source entry by (name, platform)
    if let Some(existing) = lock
        .sources
        .iter_mut()
        .find(|s| s.name == last.name && s.platform == target_platform)
    {
        let next_digest = artifact_source_digest.to_string();
        let next_excludes = &last.excludes;
        let next_includes = &last.includes;
        let next_path = &last.path;

        if existing.digest != next_digest
            || existing.includes != *next_includes
            || existing.excludes != *next_excludes
            || existing.path != *next_path
        {
            existing.digest = next_digest;
            existing.excludes.clone_from(next_excludes);
            existing.includes.clone_from(next_includes);
            existing.path.clone_from(next_path);

            lockfile_modified = true;
        }
    } else {
        // `last` borrows from `artifact_sources`, which the caller still owns;
        // the new lock entry needs its own owned copies of these fields.
        lock.sources.push(LockSource {
            digest: artifact_source_digest.to_string(),
            excludes: last.excludes.clone(),
            includes: last.includes.clone(),
            name: last.name.clone(),
            path: last.path.clone(),
            platform: target_platform.to_string(),
        });

        lockfile_modified = true;
    }

    if lockfile_modified {
        lock.sources
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.digest.cmp(&b.digest)));

        if let Err(e) = save_lock(lock_path, &lock).await {
            warn!("Failed to update lockfile {}: {}", lock_path.display(), e);
        } else {
            info!("Updated lockfile with source: {}", last.name);
        }
    }

    Ok(())
}

/// Hydrates `artifact_source`'s digest from its matching lockfile entry (by name and
/// platform) when nothing else about it changed, and enforces the two fail-closed
/// gates: an existing pin cannot change, and a new remote source cannot go unpinned,
/// without `--unlock`.
fn hydrate_and_gate_source(
    artifact_source: &mut ArtifactSource,
    lock_file: Option<&Lockfile>,
    target_platform: &str,
    request_artifact_unlock: bool,
) -> Result<(), Status> {
    let lock_source = lock_file.as_ref().and_then(|lock| {
        lock.sources
            .iter()
            .find(|s| s.name == artifact_source.name && s.platform == target_platform)
    });

    if let Some(lock_source) = lock_source {
        let changed_digest = artifact_source
            .digest
            .as_ref()
            .is_some_and(|digest| *digest != lock_source.digest);

        let changed_includes = artifact_source.includes != lock_source.includes;

        let changed_excludes = artifact_source.excludes != lock_source.excludes;

        let changed_path = artifact_source.path != lock_source.path;

        let changed_source = changed_digest || changed_includes || changed_excludes || changed_path;

        if changed_source && !request_artifact_unlock {
            return Err(Status::failed_precondition(format!(
                "source '{}' changed - use '--unlock' to update",
                artifact_source.name
            )));
        }

        if !changed_source && !lock_source.digest.is_empty() {
            // lock_source borrows from lock_file; artifact_source needs its own
            // owned copy, and lock_source.digest is still used in the log below.
            artifact_source.digest = Some(lock_source.digest.clone());

            info!(
                "agent |> hydrated source: {} ({}) -> {}",
                artifact_source.name, target_platform, lock_source.digest
            );
        }
    }

    // Fail-closed: reject an unpinned remote source (no inline digest, no
    // lock entry) unless --unlock is passed to mint the first pin.
    if requires_unlock_to_pin(
        &classify_source_type(&artifact_source.path),
        artifact_source.digest.is_some(),
        lock_source.is_some_and(|s| !s.digest.is_empty()),
    ) && !request_artifact_unlock
    {
        return Err(Status::failed_precondition(format!(
            "source '{}' is unpinned - use '--unlock' to pin",
            artifact_source.name
        )));
    }

    Ok(())
}

/// Resolves one artifact source: hydrates its digest from the lockfile when unchanged,
/// enforces the unlock-to-pin/unlock-to-change gates, resolves (from cache or by
/// building) its digest, appends the resolved source to `artifact_sources`, and upserts
/// an HTTP source's entry into the on-disk lockfile.
#[expect(
    clippy::too_many_arguments,
    reason = "each argument is independent state threaded through resolution (request context, target platform, caches, and the accumulator); grouping them would need a bespoke struct with no reuse beyond this call site"
)]
async fn resolve_and_upsert_source(
    mut artifact_source: ArtifactSource,
    request_artifact_context: &str,
    request_artifact_namespace: &str,
    request_artifact_unlock: bool,
    request_registry: &ResolvedRegistry,
    lock_path: &Path,
    lock_file: Option<&Lockfile>,
    target_platform: &str,
    source_cache: &SourceCache,
    artifact_sources: &mut Vec<ArtifactSource>,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
    source_builder: &dyn SourceBuilder,
) -> Result<(), Status> {
    hydrate_and_gate_source(
        &mut artifact_source,
        lock_file,
        target_platform,
        request_artifact_unlock,
    )?;

    // Both cache_key and url_cache_key are independently owned: cache_key is used
    // (and moved) at L786/804 while artifact_source is still needed afterward, and
    // url_cache_key is moved separately into by_url at L806.
    let cache_key = SourceCacheKey {
        artifact_namespace: request_artifact_namespace.to_string(),
        excludes: artifact_source.excludes.clone(),
        includes: artifact_source.includes.clone(),
        name: artifact_source.name.clone(),
        path: artifact_source.path.clone(),
        platform: target_platform.to_string(),
    };

    let is_http_source =
        artifact_source.path.starts_with("http://") || artifact_source.path.starts_with("https://");
    let url_cache_key = if is_http_source {
        Some((
            request_artifact_namespace.to_string(),
            artifact_source.path.clone(),
            artifact_source.excludes.clone(),
            artifact_source.includes.clone(),
            target_platform.to_string(),
        ))
    } else {
        None
    };

    // Only cache HTTP sources — local sources must always re-read from disk
    let cached_digest = if is_http_source {
        let cache = source_cache.lock().await;
        cache.by_key.get(&cache_key).cloned().or_else(|| {
            url_cache_key
                .as_ref()
                .and_then(|key| cache.by_url.get(key).cloned())
        })
    } else {
        None
    };

    let artifact_source_digest = if let Some(digest) = cached_digest {
        info!(
            "agent |> cache hit for source: {} ({}) -> {}",
            artifact_source.name, target_platform, digest
        );
        // Backfill the full key if the hit came from the URL cache. `digest` is
        // also this branch's return value, so the cache needs its own copy.
        source_cache
            .lock()
            .await
            .by_key
            .entry(cache_key)
            .or_insert(digest.clone());
        digest
    } else {
        let digest = source_builder
            .build(
                request_artifact_context.to_string(),
                request_artifact_namespace.to_string(),
                &artifact_source,
                request_artifact_unlock,
                request_registry,
                tx,
            )
            .await
            .map_err(|err| Status::internal(format!("{err}")))?;

        // Only populate cache for HTTP sources. `digest` is inserted into up to
        // two caches and is still this branch's return value, so each insert
        // needs its own owned copy.
        if is_http_source {
            let mut cache = source_cache.lock().await;
            cache.by_key.insert(cache_key, digest.clone());
            if let Some(url_key) = url_cache_key {
                cache.by_url.insert(url_key, digest.clone());
            }
        }

        digest
    };

    // artifact_source_digest is still needed below (upsert_source_lockfile)
    let artifact_source = ArtifactSource {
        digest: Some(artifact_source_digest.clone()),
        excludes: artifact_source.excludes,
        includes: artifact_source.includes,
        name: artifact_source.name,
        path: artifact_source.path,
    };

    artifact_sources.push(artifact_source);

    // Upsert remote source into Vorpal.lock immediately after preparation

    let is_http = artifact_sources
        .last()
        .is_some_and(|s| s.path.starts_with("http://") || s.path.starts_with("https://"));

    if is_http {
        upsert_source_lockfile(
            lock_path,
            artifact_sources,
            &artifact_source_digest,
            target_platform,
        )
        .await?;
    }

    Ok(())
}

async fn prepare_artifact(
    request: Request<PrepareArtifactRequest>,
    tx: &Sender<Result<PrepareArtifactResponse, Status>>,
    source_cache: SourceCache,
    registry_allowed: &[String],
    source_builder: &dyn SourceBuilder,
) -> Result<(), Status> {
    let request = request.into_inner();

    let Some(artifact) = request.artifact else {
        return Err(Status::invalid_argument("'artifact' is required"));
    };

    // The agent dials `request.registry` for every remote source the same
    // way the worker dials it for every dependency pull — the caller must
    // not be able to name that registry any more here than there (C1/C2 in
    // the threat model). Resolve it once, ahead of the source loop, so a
    // request naming a registry outside the allow-list is refused before
    // any network I/O rather than per-source.
    let registry = resolve_registry(&request.registry, registry_allowed)?;

    // TODO: Check if artifact already exists in the registry

    let public_key_path = get_key_service_public_path();

    let mut artifact_steps = vec![];

    for step in artifact.steps {
        let mut secrets = vec![];

        for secret in step.secrets {
            let value = notary::encrypt(&public_key_path, secret.value)
                .await
                .map_err(|err| Status::internal(format!("failed to encrypt secret: {err}")))?;

            secrets.push(ArtifactStepSecret {
                name: secret.name,
                value,
            });
        }

        artifact_steps.push(ArtifactStep {
            arguments: step.arguments,
            artifacts: step.artifacts,
            entrypoint: step.entrypoint,
            environments: step.environments,
            script: step.script,
            secrets,
        });
    }

    // Load lockfile to hydrate source digests before processing
    let lock_path = Path::new(&request.artifact_context).join("Vorpal.lock");
    let lock_file = load_lock(&lock_path).await.unwrap_or(None);

    let mut artifact_sources = vec![];

    let target_platform = artifact_system_to_platform(artifact.target);

    for artifact_source in artifact.sources {
        resolve_and_upsert_source(
            artifact_source,
            &request.artifact_context,
            &request.artifact_namespace,
            request.artifact_unlock,
            &registry,
            &lock_path,
            lock_file.as_ref(),
            &target_platform,
            &source_cache,
            &mut artifact_sources,
            tx,
            source_builder,
        )
        .await?;
    }

    // TODO: explore using combined sources digest for the artifact

    // Store artifact in the registry

    let artifact = Artifact {
        aliases: artifact.aliases,
        name: artifact.name,
        sources: artifact_sources,
        steps: artifact_steps,
        systems: artifact.systems,
        target: artifact.target,
    };

    let artifact_json =
        serde_json::to_vec(&artifact).map_err(|err| Status::internal(format!("{err}")))?;

    let artifact_digest = digest(artifact_json);

    info!(
        "agent |> prepared artifact: {} ({})",
        artifact.name, artifact_digest
    );

    let artifact_response = PrepareArtifactResponse {
        artifact: Some(artifact),
        artifact_digest: Some(artifact_digest),
        artifact_output: None,
    };

    let _ = tx
        .send(Ok(artifact_response))
        .await
        .map_err(|_| Status::internal("failed to send response"));

    Ok(())
}

#[derive(Debug)]
pub struct AgentServer {
    /// The operator-configured registry allow-list a remote source may be
    /// pulled through — same set, same fail-closed default, and the same
    /// `resolve_registry` seam the worker uses, both imported from
    /// `cli/src/command/start.rs` (the parent module they share); the agent
    /// dials a caller-supplied registry too, so it needs the identical
    /// check.
    registry_allowed: Vec<String>,
    source_cache: SourceCache,
}

impl AgentServer {
    pub fn new(registry_allowed: Vec<String>) -> Self {
        Self {
            registry_allowed,
            source_cache: Arc::new(Mutex::new(SourceCacheState::default())),
        }
    }
}

#[tonic::async_trait]
impl AgentService for AgentServer {
    type PrepareArtifactStream = ReceiverStream<Result<PrepareArtifactResponse, Status>>;

    async fn prepare_artifact(
        &self,
        request: Request<PrepareArtifactRequest>,
    ) -> Result<Response<Self::PrepareArtifactStream>, Status> {
        // The absence of a credential must never be what authorizes a
        // request. This runs as the first statement of the handler, ahead of
        // the spawn below, so a claim-free caller is refused as the RPC
        // status itself and never reaches `prepare_artifact` — which reads
        // caller-named filesystem paths and pushes what it reads into a
        // caller-named registry namespace under the host user's own stored
        // bearer. `require_namespace_or_service_trust` returns
        // `unauthenticated` when no `PrincipalKind` sits in the request
        // extensions; `authorize_namespace_if_authenticated` returns `Ok` in
        // that case and would leave the anonymous path open.
        auth::require_namespace_or_service_trust(
            &request,
            &request.get_ref().artifact_namespace,
            "write",
        )?;

        let (tx, rx) = channel(100);
        // Cloned so the spawned task can own a handle while `self` keeps its own.
        let source_cache = Arc::clone(&self.source_cache);
        let registry_allowed = self.registry_allowed.clone();

        tokio::spawn(async move {
            if let Err(err) = prepare_artifact(
                request,
                &tx,
                source_cache,
                &registry_allowed,
                &RegistrySourceBuilder,
            )
            .await
            {
                let _ = tx.send(Err(err)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Records the namespace each prepare actually reached the push-bearing
    /// builder under. A cache hit skips this call entirely, so the recorded
    /// namespaces are exactly the set of namespaces this agent pushed into.
    struct RecordingSourceBuilder {
        calls: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl RecordingSourceBuilder {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn namespaces(&self) -> Vec<String> {
            self.calls
                .lock()
                .await
                .iter()
                .map(|(namespace, _)| namespace.clone())
                .collect()
        }
    }

    #[tonic::async_trait]
    impl SourceBuilder for RecordingSourceBuilder {
        async fn build(
            &self,
            _artifact_context: String,
            artifact_namespace: String,
            artifact_source: &ArtifactSource,
            _artifact_unlock: bool,
            _registry: &ResolvedRegistry,
            _tx: &Sender<Result<PrepareArtifactResponse, Status>>,
        ) -> Result<String> {
            self.calls
                .lock()
                .await
                .push((artifact_namespace, artifact_source.path.clone()));

            Ok(SOURCE_DIGEST.to_string())
        }
    }

    const SOURCE_URL: &str = "https://example.com/source.tar.gz";
    const SOURCE_DIGEST: &str = "e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1";

    /// A request for one pinned http source, prepared under `namespace` with a
    /// context of its own. Every prepare needs a fresh context: the handler
    /// upserts `Vorpal.lock` there, and a shared context would hydrate the
    /// digest and change what the next prepare is deciding.
    fn http_source_request(namespace: &str, context: &TempDir) -> PrepareArtifactRequest {
        PrepareArtifactRequest {
            artifact: Some(Artifact {
                aliases: vec![],
                name: "test".to_string(),
                sources: vec![ArtifactSource {
                    digest: Some(SOURCE_DIGEST.to_string()),
                    excludes: vec![],
                    includes: vec![],
                    name: "source".to_string(),
                    path: SOURCE_URL.to_string(),
                }],
                steps: vec![],
                systems: vec![],
                target: 0,
            }),
            artifact_context: context.path().display().to_string(),
            artifact_namespace: namespace.to_string(),
            artifact_unlock: false,
            registry: "http://registry.example.com".to_string(),
        }
    }

    async fn prepare_http_source(
        namespace: &str,
        source_cache: SourceCache,
        builder: &RecordingSourceBuilder,
    ) {
        let (tx, _rx) = channel(100);
        let context = TempDir::new().expect("context dir");

        prepare_artifact(
            Request::new(http_source_request(namespace, &context)),
            &tx,
            source_cache,
            &["http://registry.example.com".to_string()],
            builder,
        )
        .await
        .expect("a pinned http source prepares");
    }

    // The write gate authorizes a namespace; the cache decides whether the
    // push that lands in it ever runs. A digest resolved under `x` must not
    // stand in for a prepare under `y`, or `y`'s artifact names an archive
    // only `x` holds.
    #[tokio::test]
    async fn prepare_artifact_pushes_the_same_source_once_per_namespace() {
        let source_cache: SourceCache = Arc::new(Mutex::new(SourceCacheState::default()));
        let builder = RecordingSourceBuilder::new();

        prepare_http_source("x", source_cache.clone(), &builder).await;
        prepare_http_source("y", source_cache.clone(), &builder).await;

        assert_eq!(
            builder.namespaces().await,
            vec!["x".to_string(), "y".to_string()],
            "each namespace must get its own push, not a hit on the other's entry"
        );
    }

    // The benign case the fix must not break: within one namespace the cache
    // still answers, so a second prepare of the same source does not push
    // again. Without this, deleting the cache would satisfy the test above.
    #[tokio::test]
    async fn prepare_artifact_reuses_a_cached_source_within_one_namespace() {
        let source_cache: SourceCache = Arc::new(Mutex::new(SourceCacheState::default()));
        let builder = RecordingSourceBuilder::new();

        prepare_http_source("x", source_cache.clone(), &builder).await;
        prepare_http_source("x", source_cache.clone(), &builder).await;

        assert_eq!(
            builder.namespaces().await,
            vec!["x".to_string()],
            "a repeat prepare in the same namespace must hit the cache"
        );
    }

    #[test]
    fn classify_local_when_path_exists() {
        // "." always exists on disk -> workspace-trust (local) domain.
        assert_eq!(classify_source_type("."), ArtifactSourceType::Local);
    }

    #[test]
    fn classify_remote_and_unknown_schemes() {
        assert_eq!(
            classify_source_type("https://example.com/x.tar.gz"),
            ArtifactSourceType::Http
        );
        assert_eq!(
            classify_source_type("http://example.com/x.tar.gz"),
            ArtifactSourceType::Http
        );
        assert_eq!(
            classify_source_type("git+https://example.com/r.git"),
            ArtifactSourceType::Git
        );
        assert_eq!(
            classify_source_type("ftp://example.com/x"),
            ArtifactSourceType::Unknown
        );
    }

    // AC (a): new remote source, no inline digest, no lock entry -> needs --unlock.
    #[test]
    fn remote_unpinned_requires_unlock() {
        assert!(requires_unlock_to_pin(
            &ArtifactSourceType::Http,
            false,
            false
        ));
        assert!(requires_unlock_to_pin(
            &ArtifactSourceType::Git,
            false,
            false
        ));
    }

    // An inline digest pins a remote source (verified downstream at the digest check).
    #[test]
    fn remote_with_inline_digest_allowed() {
        assert!(!requires_unlock_to_pin(
            &ArtifactSourceType::Http,
            true,
            false
        ));
    }

    // AC (c): a minted lock entry exempts subsequent builds from the mint gate.
    #[test]
    fn remote_with_lock_entry_allowed() {
        assert!(!requires_unlock_to_pin(
            &ArtifactSourceType::Http,
            false,
            true
        ));
    }

    // A (name,platform) lock entry whose digest is "" is not a usable pin: the
    // call site computes has_lock_entry via is_some_and(|s| !s.digest.is_empty()),
    // which is false for an empty digest, so the gate still requires --unlock.
    #[test]
    fn remote_with_empty_digest_lock_entry_still_requires_unlock() {
        // has_lock_entry=false mirrors what the call site computes for digest=""
        assert!(requires_unlock_to_pin(
            &ArtifactSourceType::Http,
            false,
            false
        ));
    }

    // AC (d) regression: a local source with no lock entry never needs --unlock.
    #[test]
    fn local_source_never_requires_unlock() {
        assert!(!requires_unlock_to_pin(
            &ArtifactSourceType::Local,
            false,
            false
        ));
        assert!(!requires_unlock_to_pin(
            &ArtifactSourceType::Unknown,
            false,
            false
        ));
    }

    // C2 (reconcile): the agent dials `request.registry` for every remote
    // source exactly like the worker dials it for every dependency, so it
    // needs the identical allow-list check. The refusal must happen before
    // `request.artifact` is even unwrapped, matching the worker's own
    // "resolve before touching anything else" ordering.
    #[tokio::test]
    async fn prepare_artifact_refuses_a_registry_outside_the_allow_list() {
        let (tx, _rx) = channel(100);
        let source_cache: SourceCache = Arc::new(Mutex::new(SourceCacheState::default()));

        let request = Request::new(PrepareArtifactRequest {
            artifact: Some(Artifact {
                aliases: vec![],
                name: "test".to_string(),
                sources: vec![],
                steps: vec![],
                systems: vec![],
                target: 0,
            }),
            artifact_context: ".".to_string(),
            artifact_namespace: "library".to_string(),
            artifact_unlock: false,
            registry: "http://attacker.example.com".to_string(),
        });

        let status = prepare_artifact(
            request,
            &tx,
            source_cache,
            &["http://registry.example.com".to_string()],
            &RecordingSourceBuilder::new(),
        )
        .await
        .expect_err("a registry outside the allow-list is refused");

        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(
            status.message().contains("registry"),
            "{}",
            status.message()
        );
    }

    // Positive control: an allow-listed registry passes the check and
    // proceeds to process the (empty) artifact rather than being refused.
    #[tokio::test]
    async fn prepare_artifact_accepts_an_allow_listed_registry() {
        let (tx, _rx) = channel(100);
        let source_cache: SourceCache = Arc::new(Mutex::new(SourceCacheState::default()));

        let request = Request::new(PrepareArtifactRequest {
            artifact: Some(Artifact {
                aliases: vec![],
                name: "test".to_string(),
                sources: vec![],
                steps: vec![],
                systems: vec![],
                target: 0,
            }),
            artifact_context: ".".to_string(),
            artifact_namespace: "library".to_string(),
            artifact_unlock: false,
            registry: "http://registry.example.com".to_string(),
        });

        let result = prepare_artifact(
            request,
            &tx,
            source_cache,
            &["http://registry.example.com".to_string()],
            &RecordingSourceBuilder::new(),
        )
        .await;

        // With no sources and no steps this succeeds outright; the point of
        // this control is that it is not the registry refusal above.
        if let Err(status) = result {
            assert_ne!(status.code(), Code::InvalidArgument);
            assert!(
                !status.message().contains("registry"),
                "{}",
                status.message()
            );
        }
    }

    fn prepare_request(namespace: &str) -> PrepareArtifactRequest {
        PrepareArtifactRequest {
            artifact: Some(Artifact {
                aliases: vec![],
                name: "test".to_string(),
                sources: vec![],
                steps: vec![],
                systems: vec![],
                target: 0,
            }),
            artifact_context: ".".to_string(),
            artifact_namespace: namespace.to_string(),
            artifact_unlock: false,
            registry: "http://registry.example.com".to_string(),
        }
    }

    // The trait-level handler must deny a claim-free request outright: the
    // agent had no interceptor and no namespace check at all, so an
    // anonymous peer reached a handler that reads caller-named paths and
    // pushes them under the host user's bearer.
    #[tokio::test]
    async fn prepare_artifact_service_denies_a_request_with_no_claims() {
        let server = AgentServer::new(vec!["http://registry.example.com".to_string()]);

        let status = server
            .prepare_artifact(Request::new(prepare_request("library")))
            .await
            .expect_err("a claim-free request must be denied, not silently skipped");

        assert_eq!(status.code(), Code::Unauthenticated);
    }

    // The denial must land before the allow-list check the spawned task
    // performs, so an unauthenticated caller cannot tell an allow-listed
    // registry from a rejected one by the code it gets back.
    #[tokio::test]
    async fn prepare_artifact_denies_a_claim_free_request_for_any_registry() {
        let server = AgentServer::new(vec!["http://registry.example.com".to_string()]);

        let mut request = prepare_request("library");
        request.registry = "http://attacker.example.com".to_string();

        let status = server
            .prepare_artifact(Request::new(request))
            .await
            .expect_err("a claim-free request must be denied");

        assert_eq!(status.code(), Code::Unauthenticated);
    }

    // Positive control: a request carrying `Claims`/`PrincipalKind` with
    // namespace write permission passes the gate, proving the denials above
    // are the gate firing rather than an inert fixture.
    #[tokio::test]
    async fn prepare_artifact_service_admits_a_request_with_namespace_write_claims() {
        let server = AgentServer::new(vec!["http://registry.example.com".to_string()]);

        let mut request = Request::new(prepare_request("library"));

        let mut namespaces = HashMap::new();
        namespaces.insert("library".to_string(), vec!["write".to_string()]);

        request.extensions_mut().insert(auth::Claims {
            aud: None,
            exp: None,
            iss: None,
            sub: Some("tester".to_string()),
            scope: None,
            azp: None,
            gty: None,
            namespaces: Some(namespaces),
        });
        request.extensions_mut().insert(auth::PrincipalKind::Human);

        server
            .prepare_artifact(request)
            .await
            .expect("a claims-bearing request with namespace write permission is admitted");
    }
}
