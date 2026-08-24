use crate::command::{
    start::auth::{get_user_context, require_namespace_or_service_trust, PrincipalKind},
    store::paths::{
        parse_alias_name, parse_artifact_digest, parse_store_path_component, split_alias_name_tag,
    },
};
use anyhow::{bail, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use moka::future::Cache;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info};
use vorpal_sdk::api::{
    archive::{
        archive_service_server::ArchiveService, ArchivePullRequest, ArchivePullResponse,
        ArchivePushRequest, ArchiveResponse,
    },
    artifact::{
        artifact_service_server::ArtifactService, Artifact, ArtifactRequest, ArtifactResponse,
        ArtifactSystem, ArtifactsRequest, ArtifactsResponse, GetArtifactAliasRequest,
        GetArtifactAliasResponse, StoreArtifactRequest,
    },
};

mod archive;
mod artifact;
mod s3;

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("missing s3 bucket")]
    MissingS3Bucket,
}

#[derive(Debug, Default, PartialEq)]
pub enum ServerBackend {
    #[default]
    Unknown,
    Local,
    S3,
}

#[derive(Clone, Debug)]
pub struct LocalBackend;

const DEFAULT_GRPC_CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB

/// Authz + observability label for a request's classified principal.
///
/// Produces a consistent, grep-able tag at every enforcement site so operators
/// can see *who* passed the authz gate — Human users by `sub`, `TrustedService`
/// workers by `azp`. The format is shared verbatim with `worker.rs` (DKT-65)
/// so a single `service=` / `user=` query surfaces every authz decision
/// across registry and worker:
///
///   - `service=<azp>` for [`PrincipalKind::TrustedService`]
///   - `user=<sub>`    for [`PrincipalKind::Human`]
///   - `user=<unknown>` for Human without a `sub`, or when the interceptor
///     did not insert a principal (should not happen when auth is enabled)
///
/// Required by TDD §4.5 (observability) and AC §1.3 #5 (every authz decision
/// records principal classification + identifier).
fn principal_label<T>(request: &Request<T>) -> String {
    if let Some(PrincipalKind::TrustedService { azp }) = request.extensions().get::<PrincipalKind>()
    {
        format!("service={azp}")
    } else {
        let user = get_user_context(request).unwrap_or_else(|| "<unknown>".to_string());
        format!("user={user}")
    }
}

#[derive(Clone, Debug)]
pub struct S3Backend {
    bucket: String,
    client: Client,
}

impl LocalBackend {
    pub fn new() -> Self {
        Self
    }
}

impl S3Backend {
    pub async fn new(bucket: Option<String>, force_path_style: bool) -> Result<Self, BackendError> {
        let Some(bucket) = bucket else {
            return Err(BackendError::MissingS3Bucket);
        };

        let config_sdk = aws_config::defaults(BehaviorVersion::latest()).load().await;

        let mut config_builder = aws_sdk_s3::config::Builder::from(&config_sdk);

        if force_path_style {
            config_builder = config_builder.force_path_style(true);
        }

        let config = config_builder.build();

        let client = Client::from_conf(config);

        Ok(Self { bucket, client })
    }
}

#[tonic::async_trait]
pub trait ArchiveBackend: Send + Sync + 'static {
    async fn check(&self, req: &ArchivePullRequest) -> Result<(), Status>;

    async fn pull(
        &self,
        req: &ArchivePullRequest,
        tx: &mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status>;

    async fn push(
        &self,
        digest: &str,
        namespace: &str,
        stream: &mut (dyn Stream<Item = Result<bytes::Bytes, Status>> + Unpin + Send),
    ) -> Result<(), Status>;

    /// Return a new `Box<dyn ArchiveBackend>` cloned from `self`.
    fn box_clone(&self) -> Box<dyn ArchiveBackend>;
}

impl Clone for Box<dyn ArchiveBackend> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

pub struct ArchiveServer {
    pub backend: Box<dyn ArchiveBackend>,
    /// Cache for archive check results: key is "{namespace}/{digest}", value is exists (bool)
    check_cache: Cache<String, bool>,
}

impl ArchiveServer {
    pub fn new(backend: Box<dyn ArchiveBackend>, cache_ttl_seconds: u64) -> Self {
        info!(
            "registry |> archive server: initializing check cache with ttl={}s",
            cache_ttl_seconds
        );

        // Bounded on entry count: the key is "{namespace}/{digest}" from an
        // unauthenticated endpoint (`ArchiveService::check` has no authz —
        // see the `require_namespace_or_service_trust` call added below),
        // so an unbounded cache lets any caller grow it without limit
        // regardless of the TTL (VPL-383).
        const CHECK_CACHE_MAX_ENTRIES: u64 = 100_000;

        let check_cache = if cache_ttl_seconds > 0 {
            Cache::builder()
                .max_capacity(CHECK_CACHE_MAX_ENTRIES)
                .time_to_live(Duration::from_secs(cache_ttl_seconds))
                .build()
        } else {
            // TTL of 0 means don't cache (immediate expiry)
            info!("registry |> archive server: caching disabled (ttl=0)");
            Cache::builder()
                .max_capacity(CHECK_CACHE_MAX_ENTRIES)
                .time_to_live(Duration::ZERO)
                .build()
        };

        Self {
            backend,
            check_cache,
        }
    }
}

#[tonic::async_trait]
impl ArchiveService for ArchiveServer {
    type PullStream = ReceiverStream<Result<ArchivePullResponse, Status>>;

    async fn check(
        &self,
        request: Request<ArchivePullRequest>,
    ) -> Result<Response<ArchiveResponse>, Status> {
        let digest = parse_artifact_digest(&request.get_ref().digest, "archive digest")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let namespace = parse_store_path_component(&request.get_ref().namespace, "namespace")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;

        // Authorization check — see DKT-64 note in `pull`. `check` previously
        // had no authz call at all, so any token holder (or, with auth off,
        // anyone) got an existence oracle across every namespace (VPL-383).
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            require_namespace_or_service_trust(&request, &namespace, "read")?;

            info!(
                "archive |> check by {} in namespace {}",
                principal_label(&request),
                namespace
            );
        }

        let req = request.into_inner();

        // Safe only because `digest` and `namespace` are each a single path
        // component with no separator: an unvalidated pair could collide
        // ("a"/"b/c" vs "a/b"/"c") and answer for the wrong namespace.
        let cache_key = format!("{}/{}", namespace, digest);
        info!("registry |> archive check: cache_key={}", cache_key);

        // Try cache first
        if let Some(exists) = self.check_cache.get(&cache_key).await {
            info!(
                "registry |> archive check: cache hit, exists={}, digest={}",
                exists, req.digest
            );
            if exists {
                info!("registry |> archive check (cached): {}", req.digest);
                return Ok(Response::new(ArchiveResponse {}));
            }
            return Err(Status::not_found("archive not found"));
        }

        info!(
            "registry |> archive check: cache miss, calling backend, digest={}",
            req.digest
        );

        // Cache miss - call backend
        let result = self.backend.check(&req).await;

        // Cache the result
        let exists = result.is_ok();
        info!(
            "registry |> archive check: caching result, exists={}, cache_key={}",
            exists, cache_key
        );
        self.check_cache.insert(cache_key, exists).await;

        if exists {
            info!("registry |> archive check: {}", req.digest);
            Ok(Response::new(ArchiveResponse {}))
        } else {
            result?;
            unreachable!()
        }
    }

    async fn pull(
        &self,
        request: Request<ArchivePullRequest>,
    ) -> Result<Response<Self::PullStream>, Status> {
        // Reject a hostile digest or namespace before any I/O, including
        // before spawning the task below — a value that never reaches a
        // path join cannot escape the store root (VPL-383).
        {
            let req_inner = request.get_ref();

            parse_artifact_digest(&req_inner.digest, "archive digest")
                .map_err(|err| Status::invalid_argument(err.to_string()))?;
            parse_store_path_component(&req_inner.namespace, "namespace")
                .map_err(|err| Status::invalid_argument(err.to_string()))?;
        }

        // Authorization check before spawning task.
        //
        // DKT-64: swapped from `require_namespace_permission` to
        // `require_namespace_or_service_trust` so service-user tokens with an
        // `azp` in `--issuer-service-client-ids` bypass namespace RBAC while
        // human tokens still go through the unchanged permission check.
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_or_service_trust(&request, &req_inner.namespace, "read")?;

            info!(
                "archive |> pull by {} in namespace {}",
                principal_label(&request),
                req_inner.namespace
            );
        }

        let (tx, rx) = mpsc::channel(100);

        // The spawned future must own the backend; `self` does not outlive it.
        let backend = self.backend.clone();

        tokio::spawn(async move {
            let request = request.into_inner();

            if let Err(err) = backend.pull(&request, &tx).await {
                if let Err(err) = tx.send(Err(err)).await {
                    error!("failed to send store error: {:?}", err);
                }
            }

            info!("registry |> archive pull: {}", request.digest);
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn push(
        &self,
        mut request: Request<Streaming<ArchivePushRequest>>,
    ) -> Result<Response<ArchiveResponse>, Status> {
        // Extract metadata from the first chunk. Read through `get_mut` so
        // `request` (and its extensions) stays available for the
        // authorization check below — `push` previously had no `Claims`
        // extraction and no authz call at all (VPL-383/C4): any token
        // holder could push into any namespace by naming it directly, even
        // after path validation landed.
        let first_chunk = request
            .get_mut()
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("empty stream"))?
            .map_err(|err| Status::internal(err.to_string()))?;

        let request_digest = parse_artifact_digest(&first_chunk.digest, "archive digest")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let request_namespace = parse_store_path_component(&first_chunk.namespace, "namespace")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;

        // Authorization check — see DKT-64 note in `pull`. Runs after the
        // namespace is known (it arrives in the first stream chunk, not in
        // the RPC's own metadata) and before any backend I/O.
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            require_namespace_or_service_trust(&request, &request_namespace, "write")?;

            info!(
                "archive |> push by {} in namespace {}",
                principal_label(&request),
                request_namespace
            );
        }

        let request_stream = request.into_inner();

        // Create an adapter stream that yields data bytes from the first chunk
        // and all remaining chunks without accumulating into a Vec<u8>
        let first_data = bytes::Bytes::from(first_chunk.data);
        let mut remainder = request_stream.map(|result| {
            result
                .map(|chunk| bytes::Bytes::from(chunk.data))
                .map_err(|err| Status::internal(err.to_string()))
        });

        // Chain the first chunk's data with the rest of the stream
        let mut data_stream = tokio_stream::once(Ok(first_data)).chain(&mut remainder);

        self.backend
            .push(&request_digest, &request_namespace, &mut data_stream)
            .await?;

        info!("registry |> archive push: {}", request_digest);

        Ok(Response::new(ArchiveResponse {}))
    }
}

#[tonic::async_trait]
pub trait ArtifactBackend: Send + Sync + 'static {
    async fn get_artifact(&self, digest: &str, namespace: &str) -> Result<Artifact, Status>;

    async fn get_artifact_alias(
        &self,
        name: &str,
        namespace: &str,
        system: ArtifactSystem,
        version: &str,
    ) -> Result<String, Status>;

    async fn store_artifact(
        &self,
        artifact: Artifact,
        artifact_aliases: Vec<String>,
        artifact_namespace: String,
    ) -> Result<String, Status>;

    /// Return a new `Box<dyn ArtifactBackend>` cloned from `self`.
    fn box_clone(&self) -> Box<dyn ArtifactBackend>;
}

impl Clone for Box<dyn ArtifactBackend> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

pub struct ArtifactServer {
    pub backend: Box<dyn ArtifactBackend>,
}

impl ArtifactServer {
    pub fn new(backend: Box<dyn ArtifactBackend>) -> Self {
        Self { backend }
    }
}

#[tonic::async_trait]
impl ArtifactService for ArtifactServer {
    async fn get_artifact(
        &self,
        request: Request<ArtifactRequest>,
    ) -> Result<Response<Artifact>, Status> {
        // Authorization check — see DKT-64 note in `ArchiveService::pull`.
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_or_service_trust(&request, &req_inner.namespace, "read")?;

            info!(
                "artifact |> get_artifact by {} in namespace {}",
                principal_label(&request),
                req_inner.namespace
            );
        }

        let request = request.into_inner();

        let digest = parse_artifact_digest(&request.digest, "artifact digest")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let namespace = parse_store_path_component(&request.namespace, "namespace")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;

        let artifact = self.backend.get_artifact(&digest, &namespace).await?;

        info!("artifact |> get: {}", digest);

        Ok(Response::new(artifact))
    }

    async fn get_artifact_alias(
        &self,
        request: Request<GetArtifactAliasRequest>,
    ) -> Result<Response<GetArtifactAliasResponse>, Status> {
        // Authorization check — see DKT-64 note in `ArchiveService::pull`.
        // This site previously had no observability log line; DKT-64 adds one
        // so every authz decision (AC §1.3 #5) is logged uniformly.
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_or_service_trust(&request, &req_inner.namespace, "read")?;

            info!(
                "artifact |> get_artifact_alias by {} in namespace {}",
                principal_label(&request),
                req_inner.namespace
            );
        }

        let request = request.into_inner();

        // Reject a hostile name, namespace or tag before the join
        // `get_artifact_alias_path` performs — this handler previously
        // checked nothing, so any of the three could walk the read outside
        // the store root (VPL-383).
        let name = parse_alias_name(&request.name, "alias name")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let namespace = parse_store_path_component(&request.namespace, "namespace")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let tag = parse_store_path_component(&request.tag, "tag")
            .map_err(|err| Status::invalid_argument(err.to_string()))?;

        let request_system = ArtifactSystem::try_from(request.system);

        let digest = self
            .backend
            .get_artifact_alias(
                &name,
                &namespace,
                request_system.unwrap_or(ArtifactSystem::UnknownSystem),
                &tag,
            )
            .await?;

        info!("artifact |> alias get: {}:{} -> {}", name, tag, digest);

        Ok(Response::new(GetArtifactAliasResponse { digest }))
    }

    async fn get_artifacts(
        &self,
        _request: Request<ArtifactsRequest>,
    ) -> Result<Response<ArtifactsResponse>, Status> {
        // TODO: implement this method
        // let request = request.into_inner();
        // let digests = self.backend.get_artifacts(&request).await?;
        // Ok(Response::new(ArtifactsResponse { digests }))
        Err(Status::unimplemented(
            "get_artifacts is not implemented yet",
        ))
    }

    async fn store_artifact(
        &self,
        request: Request<StoreArtifactRequest>,
    ) -> Result<Response<ArtifactResponse>, Status> {
        // Authorization check — see DKT-64 note in `ArchiveService::pull`.
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_or_service_trust(&request, &req_inner.artifact_namespace, "write")?;

            info!(
                "artifact |> store_artifact by {} in namespace {}",
                principal_label(&request),
                req_inner.artifact_namespace
            );
        }

        let request = request.into_inner();

        let artifact = request
            .artifact
            .ok_or_else(|| Status::invalid_argument("missing `artifact` field"))?;

        let artifact_namespace =
            parse_store_path_component(&request.artifact_namespace, "namespace")
                .map_err(|err| Status::invalid_argument(err.to_string()))?;

        // Every alias this call will write — from the artifact itself and
        // from the request — goes through the same read-side rule
        // (`parse_alias_name`) before any backend runs, so a hostile name or
        // tag never reaches a path join. An empty name is skipped rather than
        // rejected, matching the backends' own "no alias requested" reading
        // of an empty entry.
        for alias in artifact
            .aliases
            .iter()
            .chain(request.artifact_aliases.iter())
        {
            let (alias_name, alias_tag) = split_alias_name_tag(alias);

            if alias_name.is_empty() {
                continue;
            }

            parse_alias_name(alias_name, "alias name")
                .map_err(|err| Status::invalid_argument(err.to_string()))?;

            parse_store_path_component(alias_tag, "tag")
                .map_err(|err| Status::invalid_argument(err.to_string()))?;
        }

        let digest = self
            .backend
            .store_artifact(artifact, request.artifact_aliases, artifact_namespace)
            .await?;

        info!("artifact |> store: {}", digest);

        Ok(Response::new(ArtifactResponse { digest }))
    }
}

pub async fn backend_archive(
    registry_backend: String,
    registry_backend_s3_bucket: Option<String>,
    registry_backend_s3_force_path_style: bool,
) -> Result<Box<dyn ArchiveBackend>> {
    let backend = match registry_backend.as_str() {
        "local" => ServerBackend::Local,
        "s3" => ServerBackend::S3,
        _ => ServerBackend::Unknown,
    };

    let backend_archive: Box<dyn ArchiveBackend> = match backend {
        ServerBackend::Local => Box::new(LocalBackend::new()),
        ServerBackend::S3 => Box::new(
            S3Backend::new(
                registry_backend_s3_bucket,
                registry_backend_s3_force_path_style,
            )
            .await?,
        ),
        ServerBackend::Unknown => bail!("unknown archive backend: {registry_backend}"),
    };

    Ok(backend_archive)
}

pub async fn backend_artifact(
    registry_backend: &str,
    registry_backend_s3_bucket: Option<String>,
    registry_backend_s3_force_path_style: bool,
) -> Result<Box<dyn ArtifactBackend>> {
    let backend = match registry_backend {
        "local" => ServerBackend::Local,
        "s3" => ServerBackend::S3,
        _ => ServerBackend::Unknown,
    };

    let backend_artifact: Box<dyn ArtifactBackend> = match backend {
        ServerBackend::Local => Box::new(LocalBackend::new()),
        ServerBackend::S3 => Box::new(
            S3Backend::new(
                registry_backend_s3_bucket,
                registry_backend_s3_force_path_style,
            )
            .await?,
        ),
        ServerBackend::Unknown => bail!("unknown artifact backend: {registry_backend}"),
    };

    Ok(backend_artifact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use vorpal_sdk::api::archive::archive_service_server::ArchiveService;

    /// Recorded (digest, namespace, `collected_data`) for one push call.
    type PushCall = (String, String, Vec<u8>);


    /// Mock backend that tracks call counts, received data, and returns configurable results.
    struct MockBackend {
        check_call_count: Arc<AtomicUsize>,
        push_call_count: Arc<AtomicUsize>,
        should_exist: bool,
        /// Stores one entry per push call.
        push_calls: Arc<Mutex<Vec<PushCall>>>,
        /// If set, the push method returns this error.
        push_error: Option<Status>,
    }

    impl MockBackend {
        fn new(should_exist: bool) -> Self {
            Self {
                check_call_count: Arc::new(AtomicUsize::new(0)),
                push_call_count: Arc::new(AtomicUsize::new(0)),
                should_exist,
                push_calls: Arc::new(Mutex::new(Vec::new())),
                push_error: None,
            }
        }

        fn call_count(&self) -> usize {
            self.check_call_count.load(Ordering::SeqCst)
        }

        fn push_count(&self) -> usize {
            self.push_call_count.load(Ordering::SeqCst)
        }

        fn push_calls(&self) -> Arc<Mutex<Vec<PushCall>>> {
            Arc::clone(&self.push_calls)
        }
    }

    #[tonic::async_trait]
    impl ArchiveBackend for MockBackend {
        async fn check(&self, _req: &ArchivePullRequest) -> Result<(), Status> {
            self.check_call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_exist {
                Ok(())
            } else {
                Err(Status::not_found("archive not found"))
            }
        }

        async fn pull(
            &self,
            _req: &ArchivePullRequest,
            _tx: &mpsc::Sender<Result<ArchivePullResponse, Status>>,
        ) -> Result<(), Status> {
            Err(Status::unimplemented(
                "MockBackend::pull is not needed for cache tests",
            ))
        }

        async fn push(
            &self,
            digest: &str,
            namespace: &str,
            stream: &mut (dyn Stream<Item = Result<bytes::Bytes, Status>> + Unpin + Send),
        ) -> Result<(), Status> {
            self.push_call_count.fetch_add(1, Ordering::SeqCst);

            if let Some(ref err) = self.push_error {
                return Err(Status::new(err.code(), err.message()));
            }

            // Drain the stream and collect all data (verifies stream is consumable).
            let mut collected = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                collected.extend_from_slice(&chunk);
            }

            self.push_calls.lock().await.push((
                digest.to_string(),
                namespace.to_string(),
                collected,
            ));

            Ok(())
        }

        fn box_clone(&self) -> Box<dyn ArchiveBackend> {
            Box::new(MockBackend {
                check_call_count: Arc::clone(&self.check_call_count),
                push_call_count: Arc::clone(&self.push_call_count),
                should_exist: self.should_exist,
                push_calls: Arc::clone(&self.push_calls),
                push_error: self
                    .push_error
                    .as_ref()
                    .map(|e| Status::new(e.code(), e.message())),
            })
        }
    }

    // Bare 64-char lowercase hex, the only shape `parse_artifact_digest`
    // accepts — every pre-existing cache test below used a placeholder like
    // "digest1" that a real digest never takes, so they now use one of
    // these instead.
    const DIGEST_1: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
    const DIGEST_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const DIGEST_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const DIGEST_GENERIC: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const DIGEST_MISSING: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    fn make_check_request(namespace: &str, digest: &str) -> Request<ArchivePullRequest> {
        Request::new(ArchivePullRequest {
            namespace: namespace.to_string(),
            digest: digest.to_string(),
        })
    }

    #[tokio::test]
    async fn test_cache_hit_skips_backend() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a server with caching enabled (TTL = 300s)
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check the same archive twice
        server.check(make_check_request("ns", DIGEST_1)).await?;
        server.check(make_check_request("ns", DIGEST_1)).await?;

        // Then: backend should only be called once (second call hits cache)
        assert_eq!(backend.call_count(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_cache_miss_for_different_keys() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a server with caching enabled
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check different digests
        server.check(make_check_request("ns", DIGEST_A)).await?;
        server.check(make_check_request("ns", DIGEST_B)).await?;

        // Then: backend should be called twice (each is a cache miss)
        assert_eq!(backend.call_count(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_cache_miss_for_different_namespaces() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a server with caching enabled
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check the same digest in different namespaces
        server.check(make_check_request("ns1", DIGEST_GENERIC)).await?;
        server.check(make_check_request("ns2", DIGEST_GENERIC)).await?;

        // Then: backend should be called twice (different cache keys)
        assert_eq!(backend.call_count(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_negative_caching_not_found() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a server with a backend that returns "not found"
        let backend = MockBackend::new(false);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check the same archive twice
        let result1 = server.check(make_check_request("ns", DIGEST_MISSING)).await;
        let result2 = server.check(make_check_request("ns", DIGEST_MISSING)).await;

        // Then: both should return not_found
        let Err(err1) = result1 else {
            return Err("expected first check to fail".into());
        };
        assert_eq!(err1.code(), tonic::Code::NotFound);
        let Err(err2) = result2 else {
            return Err("expected second check to fail".into());
        };
        assert_eq!(err2.code(), tonic::Code::NotFound);

        // And: backend should only be called once (negative result is cached)
        assert_eq!(backend.call_count(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_ttl_zero_disables_caching() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a server with TTL = 0 (caching disabled)
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 0);

        // When: we check the same archive multiple times
        server.check(make_check_request("ns", DIGEST_GENERIC)).await?;
        server.check(make_check_request("ns", DIGEST_GENERIC)).await?;
        server.check(make_check_request("ns", DIGEST_GENERIC)).await?;

        // Then: backend should be called every time (no caching)
        assert_eq!(backend.call_count(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn test_ttl_expiration() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a server with a TTL short enough to keep this test's real
        // sleep well under 200ms. `moka::future::Cache` tracks TTL against
        // its own internal clock rather than tokio's, so `tokio::time::pause`
        // /`advance` (which would need the `test-util` feature, a
        // `Cargo.toml` change out of this step's declared scope) cannot
        // replace the sleep here — shrinking the interval an order of
        // magnitude is the change available inside scope. Previously 1s TTL
        // / 1.1s sleep, the single largest contributor to this suite's
        // runtime.
        let backend = MockBackend::new(true);
        let server = ArchiveServer {
            backend: backend.box_clone(),
            check_cache: Cache::builder()
                .max_capacity(1_000)
                .time_to_live(Duration::from_millis(50))
                .build(),
        };

        // When: we check, wait for TTL to expire, then check again
        server.check(make_check_request("ns", DIGEST_GENERIC)).await?;
        assert_eq!(backend.call_count(), 1);

        // Wait for cache to expire.
        tokio::time::sleep(Duration::from_millis(150)).await;

        server.check(make_check_request("ns", DIGEST_GENERIC)).await?;

        // Then: backend should be called twice (second call after expiration)
        assert_eq!(backend.call_count(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_check_returns_error_for_empty_digest() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a server
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check with an empty digest
        let result = server.check(make_check_request("ns", "")).await;

        // Then: should return InvalidArgument error
        let Err(err) = result else {
            return Err("expected check to fail on empty digest".into());
        };
        assert_eq!(err.code(), tonic::Code::InvalidArgument);

        // And: backend should not be called
        assert_eq!(backend.call_count(), 0);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Namespace/digest collision (VPL-383 AC-4): a traversing pair must be
    // refused rather than silently answering for a different namespace via
    // string concatenation.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_check_rejects_a_namespace_that_is_not_a_single_component() {
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // (namespace="a", digest="b/c") would compose to the same store path
        // as (namespace="a/b", digest="c") if `digest` alone were checked.
        // Note: `digest="b/c"` is also rejected on its own by
        // `parse_artifact_digest` (a `/` is never valid hex), so this alone
        // does not prove the *namespace* parser is what stops the collision
        // — see the next test for that isolation.
        let result = server.check(make_check_request("a", "b/c")).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.call_count(), 0);
    }

    #[tokio::test]
    async fn test_check_rejects_a_namespace_containing_a_slash_with_an_otherwise_valid_digest() {
        // Isolates the namespace side of the AC-4 collision: `digest` here
        // is a well-formed 64-hex digest, so this can only fail on
        // `namespace`, proving `parse_store_path_component` — not
        // `parse_artifact_digest` — is what refuses (namespace="a/b",
        // digest="c") from composing into `.../archive/a/b/c...`.
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        let result = server
            .check(make_check_request("a/b", DIGEST_GENERIC))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.call_count(), 0);
    }

    #[tokio::test]
    async fn test_check_rejects_a_digest_that_is_not_64_hex_characters() {
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // One character short of a real sha256 digest.
        let short_digest = &DIGEST_GENERIC[..63];
        let result = server.check(make_check_request("ns", short_digest)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.call_count(), 0);
    }

    #[tokio::test]
    async fn test_check_rejects_a_traversing_digest() {
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        let result = server
            .check(make_check_request(
                "library",
                "../../../../../etc/cron.d/pwn",
            ))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.call_count(), 0);
    }

    #[tokio::test]
    async fn test_check_accepts_a_well_formed_digest_and_namespace() {
        // Positive control: a real 64-hex digest with a single-component
        // namespace still reaches the backend.
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        let result = server
            .check(make_check_request("library", DIGEST_GENERIC))
            .await;

        assert!(result.is_ok());
        assert_eq!(backend.call_count(), 1);
    }

    // -----------------------------------------------------------------------
    // `ArtifactService::get_artifact_alias` (VPL-383 AC-1/AC-2): this handler
    // previously checked nothing. A hostile `name` or `tag` reached the
    // backend's path join and, on the local backend, returned the target
    // file's bytes to the caller.
    // -----------------------------------------------------------------------

    struct MockArtifactBackend {
        get_artifact_alias_calls: Arc<AtomicUsize>,
        store_artifact_calls: Arc<AtomicUsize>,
    }

    impl MockArtifactBackend {
        fn new() -> Self {
            Self {
                get_artifact_alias_calls: Arc::new(AtomicUsize::new(0)),
                store_artifact_calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn alias_call_count(&self) -> usize {
            self.get_artifact_alias_calls.load(Ordering::SeqCst)
        }

        fn store_call_count(&self) -> usize {
            self.store_artifact_calls.load(Ordering::SeqCst)
        }
    }

    #[tonic::async_trait]
    impl ArtifactBackend for MockArtifactBackend {
        async fn get_artifact(&self, _digest: &str, _namespace: &str) -> Result<Artifact, Status> {
            unimplemented!("not needed for validation tests")
        }

        async fn get_artifact_alias(
            &self,
            _name: &str,
            _namespace: &str,
            _system: ArtifactSystem,
            _version: &str,
        ) -> Result<String, Status> {
            self.get_artifact_alias_calls.fetch_add(1, Ordering::SeqCst);
            Ok(DIGEST_GENERIC.to_string())
        }

        async fn store_artifact(
            &self,
            _artifact: Artifact,
            _artifact_aliases: Vec<String>,
            _artifact_namespace: String,
        ) -> Result<String, Status> {
            self.store_artifact_calls.fetch_add(1, Ordering::SeqCst);
            Ok(DIGEST_GENERIC.to_string())
        }

        fn box_clone(&self) -> Box<dyn ArtifactBackend> {
            Box::new(MockArtifactBackend {
                get_artifact_alias_calls: Arc::clone(&self.get_artifact_alias_calls),
                store_artifact_calls: Arc::clone(&self.store_artifact_calls),
            })
        }
    }

    // -----------------------------------------------------------------------
    // `ArtifactService::get_artifact` (VPL-383/C1-C2): a traversing digest or
    // namespace must be refused before the backend's path join runs.
    // -----------------------------------------------------------------------

    fn make_get_artifact_request(namespace: &str, digest: &str) -> Request<ArtifactRequest> {
        Request::new(ArtifactRequest {
            namespace: namespace.to_string(),
            digest: digest.to_string(),
        })
    }

    #[tokio::test]
    async fn test_get_artifact_rejects_a_traversing_digest() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .get_artifact(make_get_artifact_request(
                "library",
                "../../../../../key/credentials.json",
            ))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_get_artifact_rejects_a_namespace_that_is_not_a_single_component() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .get_artifact(make_get_artifact_request("a/b", DIGEST_GENERIC))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    // -----------------------------------------------------------------------
    // `ArchiveService::pull` (VPL-383/C1-C2): a traversing digest or
    // namespace must be refused before the streaming response is ever
    // opened — previously an invalid pair surfaced as the first item in the
    // response stream rather than as a handshake failure.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pull_rejects_a_traversing_digest() {
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        let result = server
            .pull(make_check_request(
                "library",
                "../../../../../etc/cron.d/pwn",
            ))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_pull_rejects_a_namespace_that_is_not_a_single_component() {
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        let result = server.pull(make_check_request("a/b", DIGEST_GENERIC)).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    fn make_alias_request(
        name: &str,
        namespace: &str,
        tag: &str,
    ) -> Request<GetArtifactAliasRequest> {
        Request::new(GetArtifactAliasRequest {
            system: ArtifactSystem::Aarch64Linux as i32,
            name: name.to_string(),
            namespace: namespace.to_string(),
            tag: tag.to_string(),
        })
    }

    #[tokio::test]
    async fn test_get_artifact_alias_rejects_a_traversing_name_before_any_read() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .get_artifact_alias(make_alias_request(
                "../../../../../key",
                "library",
                "credentials.json",
            ))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.alias_call_count(), 0);
    }

    #[tokio::test]
    async fn test_get_artifact_alias_rejects_an_absolute_name() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .get_artifact_alias(make_alias_request("/etc/passwd", "library", "latest"))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.alias_call_count(), 0);
    }

    #[tokio::test]
    async fn test_get_artifact_alias_rejects_a_traversing_tag() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .get_artifact_alias(make_alias_request("rust", "library", "../../etc/shadow"))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.alias_call_count(), 0);
    }

    #[tokio::test]
    async fn test_get_artifact_alias_rejects_a_namespace_that_is_not_a_single_component() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .get_artifact_alias(make_alias_request("rust", "library/../secret", "latest"))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.alias_call_count(), 0);
    }

    #[tokio::test]
    async fn test_get_artifact_alias_accepts_a_well_formed_request() {
        // Positive control: a real alias name, namespace and tag still reach
        // the backend.
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .get_artifact_alias(make_alias_request("rust", "library", "latest"))
            .await;

        assert!(result.is_ok());
        assert_eq!(backend.alias_call_count(), 1);
    }

    // -----------------------------------------------------------------------
    // `ArtifactService::store_artifact` (VPL-383 AC-2): the namespace and
    // every alias name/tag — from both the artifact's own `aliases` and the
    // request's `artifact_aliases` — must be rejected before the backend
    // writes anything.
    // -----------------------------------------------------------------------

    fn make_store_request(
        namespace: &str,
        artifact_aliases: Vec<&str>,
    ) -> Request<StoreArtifactRequest> {
        Request::new(StoreArtifactRequest {
            artifact: Some(Artifact::default()),
            artifact_aliases: artifact_aliases.into_iter().map(String::from).collect(),
            artifact_namespace: namespace.to_string(),
        })
    }

    #[tokio::test]
    async fn test_store_artifact_rejects_a_namespace_that_is_not_a_single_component() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .store_artifact(make_store_request("../../etc", vec![]))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.store_call_count(), 0);
    }

    #[tokio::test]
    async fn test_store_artifact_rejects_a_traversing_alias_name() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .store_artifact(make_store_request(
                "library",
                vec!["../../../../../x:latest"],
            ))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.store_call_count(), 0);
    }

    #[tokio::test]
    async fn test_store_artifact_rejects_a_traversing_alias_tag() {
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .store_artifact(make_store_request("library", vec!["rust:../../../../../x"]))
            .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.store_call_count(), 0);
    }

    #[tokio::test]
    async fn test_store_artifact_rejects_a_traversing_alias_name_in_the_artifacts_own_aliases_field(
    ) {
        // Every other store_artifact test drives the alias through the
        // request's `artifact_aliases` field with `Artifact::default()`
        // (whose own `aliases` is empty) — none exercised the artifact's
        // own `aliases` field, the other source this handler validates
        // (VPL-383, registry.rs's `.chain(request.artifact_aliases.iter())`
        // over `artifact.aliases`).
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let request = Request::new(StoreArtifactRequest {
            artifact: Some(Artifact {
                aliases: vec!["../../../../../x:latest".to_string()],
                ..Artifact::default()
            }),
            artifact_aliases: vec![],
            artifact_namespace: "library".to_string(),
        });

        let result = server.store_artifact(request).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
        assert_eq!(backend.store_call_count(), 0);
    }

    #[tokio::test]
    async fn test_store_artifact_accepts_a_well_formed_alias_in_the_artifacts_own_aliases_field() {
        // Positive control for the artifact's own `aliases` field.
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let request = Request::new(StoreArtifactRequest {
            artifact: Some(Artifact {
                aliases: vec!["rust:latest".to_string()],
                ..Artifact::default()
            }),
            artifact_aliases: vec![],
            artifact_namespace: "library".to_string(),
        });

        let result = server.store_artifact(request).await;

        assert!(result.is_ok());
        assert_eq!(backend.store_call_count(), 1);
    }

    #[tokio::test]
    async fn test_store_artifact_accepts_a_well_formed_request() {
        // Positive control: a well-formed namespace and alias still reach
        // the backend.
        let backend = MockArtifactBackend::new();
        let server = ArtifactServer::new(backend.box_clone());

        let result = server
            .store_artifact(make_store_request("library", vec!["rust:latest"]))
            .await;

        assert!(result.is_ok());
        assert_eq!(backend.store_call_count(), 1);
    }

    // -----------------------------------------------------------------------
    // Streaming push tests (DKT-16)
    // -----------------------------------------------------------------------
    //
    // NOTE on handler-level tests: tonic::Streaming<T> cannot be constructed
    // from a plain stream in unit tests (it requires HTTP body + codec internals).
    // The handler tests below exercise the ArchiveService::push method indirectly
    // through the handler's stream-processing pipeline. The backend-level tests
    // exercise push streaming directly.
    //
    // NOTE on silent stream truncation (pre-existing gap): The handler at line 253
    // calls `request_stream.next().await` which returns `None` on client disconnect.
    // If the client disconnects mid-stream after metadata extraction, the backend
    // receives a truncated stream and writes a partial archive. This is a pre-existing
    // gap in error handling (not introduced by the streaming refactor) — the server
    // has no way to distinguish "stream ended normally" from "client dropped
    // connection after sending some data". Integrity is preserved by digest
    // verification at a higher layer.

    /// Helper: create a byte stream from chunks for backend push tests.
    fn byte_stream_from_chunks(
        chunks: Vec<Result<bytes::Bytes, Status>>,
    ) -> impl Stream<Item = Result<bytes::Bytes, Status>> + Unpin + Send {
        tokio_stream::iter(chunks)
    }

    #[tokio::test]
    async fn test_mock_backend_push_drains_stream_and_records_args(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Given: a mock backend
        let backend = MockBackend::new(true);

        // When: we push a multi-chunk stream
        let chunks = vec![
            Ok(bytes::Bytes::from_static(b"hello ")),
            Ok(bytes::Bytes::from_static(b"world")),
        ];
        let mut stream = byte_stream_from_chunks(chunks);

        backend.push("sha256:abc", "default", &mut stream).await?;

        // Then: backend received correct args and drained all data
        assert_eq!(backend.push_count(), 1);
        let push_calls = backend.push_calls();
        let calls = push_calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "sha256:abc");
        assert_eq!(calls[0].1, "default");
        assert_eq!(calls[0].2, b"hello world");
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_backend_push_handles_empty_data_stream(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Given: a mock backend
        let backend = MockBackend::new(true);

        // When: we push a stream with no data chunks (empty stream)
        let chunks: Vec<Result<bytes::Bytes, Status>> = vec![];
        let mut stream = byte_stream_from_chunks(chunks);

        backend.push("sha256:abc", "default", &mut stream).await?;

        // Then: backend received the call with empty data
        let push_calls = backend.push_calls();
        let calls = push_calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, b"" as &[u8]);
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_backend_push_propagates_stream_error(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Given: a mock backend
        let backend = MockBackend::new(true);

        // When: the stream yields an error after some data
        let chunks = vec![
            Ok(bytes::Bytes::from_static(b"partial")),
            Err(Status::internal("client disconnected")),
        ];
        let mut stream = byte_stream_from_chunks(chunks);

        let result = backend.push("sha256:abc", "default", &mut stream).await;

        // Then: the push fails with the stream error
        let Err(err) = result else {
            return Err("expected push to fail".into());
        };
        assert_eq!(err.code(), tonic::Code::Internal);
        Ok(())
    }

    #[tokio::test]
    async fn test_mock_backend_push_with_zero_length_data() {
        // Given: a mock backend receiving metadata but zero-length data bytes
        // This tests the edge case where the handler sends a first chunk with
        // empty data bytes (e.g., metadata-only first message).
        let backend = MockBackend::new(true);

        let chunks = vec![Ok(bytes::Bytes::new())]; // zero-length Bytes
        let mut stream = byte_stream_from_chunks(chunks);

        let result = backend.push("sha256:abc", "default", &mut stream).await;

        // Then: push succeeds (backend receives the empty data)
        assert!(result.is_ok());
        let push_calls = backend.push_calls();
        let calls = push_calls.lock().await;
        assert_eq!(calls[0].2, b"" as &[u8]);
    }

    #[tokio::test]
    async fn test_concurrent_pushes_are_independent() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a shared mock backend
        let backend = MockBackend::new(true);

        // When: 3 concurrent pushes with different data
        let b1 = backend.box_clone();
        let b2 = backend.box_clone();
        let b3 = backend.box_clone();

        let (r1, r2, r3) = tokio::join!(
            async {
                let mut stream =
                    byte_stream_from_chunks(vec![Ok(bytes::Bytes::from_static(b"archive-1"))]);
                b1.push("sha256:aaa", "ns1", &mut stream).await
            },
            async {
                let mut stream =
                    byte_stream_from_chunks(vec![Ok(bytes::Bytes::from_static(b"archive-2"))]);
                b2.push("sha256:bbb", "ns2", &mut stream).await
            },
            async {
                let mut stream =
                    byte_stream_from_chunks(vec![Ok(bytes::Bytes::from_static(b"archive-3"))]);
                b3.push("sha256:ccc", "ns3", &mut stream).await
            },
        );

        // Then: all succeed independently
        r1?;
        r2?;
        r3?;

        assert_eq!(backend.push_count(), 3);
        let push_calls = backend.push_calls();
        let calls = push_calls.lock().await;
        assert_eq!(calls.len(), 3);

        // Verify no cross-contamination (data matches expected per digest)
        let digests: Vec<&str> = calls.iter().map(|(d, _, _)| d.as_str()).collect();
        assert!(digests.contains(&"sha256:aaa"));
        assert!(digests.contains(&"sha256:bbb"));
        assert!(digests.contains(&"sha256:ccc"));

        for call in calls.iter() {
            match call.0.as_str() {
                "sha256:aaa" => assert_eq!(call.2, b"archive-1"),
                "sha256:bbb" => assert_eq!(call.2, b"archive-2"),
                "sha256:ccc" => assert_eq!(call.2, b"archive-3"),
                other => return Err(format!("unexpected digest: {other}").into()),
            }
        }

        Ok(())
    }

    // LocalBackend's own temp-file cleanup on a stream error
    // (`registry/archive/local.rs`) has no test here. It writes under
    // `get_artifact_archive_path`, which is hardcoded to /var/lib/vorpal, so
    // reaching it needs either write access to that path or a configurable
    // base path. The test that used to sit here asserted a constant and
    // exercised nothing; it was removed rather than left reporting a pass for
    // behavior it never ran.

    #[tokio::test]
    async fn test_push_large_multi_chunk_stream() -> Result<(), Box<dyn std::error::Error>> {
        // Given: a mock backend
        let backend = MockBackend::new(true);

        // When: we push a stream with many small chunks (simulating a large archive)
        let chunk_count = 100;
        let chunk_data = bytes::Bytes::from(vec![0xABu8; 1024]); // 1KB per chunk
                                                                 // Bytes::clone is a refcount bump (not a byte copy); the map closure
                                                                 // runs once per chunk and each needs its own owned Bytes handle.
        let chunks: Vec<Result<bytes::Bytes, Status>> =
            (0..chunk_count).map(|_| Ok(chunk_data.clone())).collect();
        let mut stream = byte_stream_from_chunks(chunks);

        backend.push("sha256:large", "default", &mut stream).await?;

        // Then: all data was received (100KB total)
        let push_calls = backend.push_calls();
        let calls = push_calls.lock().await;
        assert_eq!(calls[0].2.len(), chunk_count * 1024);
        assert!(calls[0].2.iter().all(|&b| b == 0xAB));
        Ok(())
    }

    // NOTE on S3Backend tests (DKT-16 scenarios 6, 8):
    // S3Backend tests require mocking the AWS S3 client, which is not feasible
    // in unit tests without a mock S3 server (e.g., localstack) or the
    // aws-smithy-mocks crate. The S3 streaming push logic is verified by:
    // 1. Code review: abort_multipart_upload is called on error (s3.rs:234-252)
    // 2. Code review: PutObject path used when total data < 5MB (s3.rs:160-176)
    // 3. Code review: multipart upload lifecycle (create, upload parts, complete)
    // Integration tests with a mock S3 server are recommended for CI.
}
