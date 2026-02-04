use crate::command::start::auth::{get_user_context, require_namespace_permission};
use anyhow::{bail, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use moka::future::Cache;
use std::time::Duration;
use tokio::sync::mpsc;
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

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ServerBackend {
    #[default]
    Unknown,
    Local,
    S3,
}

#[derive(Clone, Debug)]
pub struct LocalBackend;

const DEFAULT_GRPC_CHUNK_SIZE: usize = 2 * 1024 * 1024; // 2MB

#[derive(Clone, Debug)]
pub struct S3Backend {
    bucket: String,
    client: Client,
}

impl LocalBackend {
    pub fn new() -> Result<Self, BackendError> {
        Ok(Self)
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
        tx: mpsc::Sender<Result<ArchivePullResponse, Status>>,
    ) -> Result<(), Status>;

    async fn push(&self, req: &ArchivePushRequest) -> Result<(), Status>;

    /// Return a new `Box<dyn RegistryBackend>` cloned from `self`.
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

        let check_cache = if cache_ttl_seconds > 0 {
            Cache::builder()
                .time_to_live(Duration::from_secs(cache_ttl_seconds))
                .build()
        } else {
            // TTL of 0 means don't cache (immediate expiry)
            info!("registry |> archive server: caching disabled (ttl=0)");
            Cache::builder()
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
        let req = request.into_inner();

        if req.digest.is_empty() {
            return Err(Status::invalid_argument("missing `digest` field"));
        }

        let cache_key = format!("{}/{}", req.namespace, req.digest);
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
            } else {
                return Err(Status::not_found("archive not found"));
            }
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
        // Authorization check before spawning task
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_permission(&request, &req_inner.namespace, "read")?;

            if let Some(user) = get_user_context(&request) {
                info!(
                    "archive |> pull requested by {} in namespace {}",
                    user, req_inner.namespace
                );
            }
        }

        let (tx, rx) = mpsc::channel(100);

        let backend = self.backend.clone();

        tokio::spawn(async move {
            let request = request.into_inner();

            if request.digest.is_empty() {
                if let Err(err) = tx
                    .send(Err(Status::invalid_argument("missing `digest` field")))
                    .await
                {
                    error!("failed to send store error: {:?}", err);
                }

                return;
            }

            if let Err(err) = backend.pull(&request, tx.clone()).await {
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
        request: Request<Streaming<ArchivePushRequest>>,
    ) -> Result<Response<ArchiveResponse>, Status> {
        let mut request_data: Vec<u8> = vec![];
        let mut request_digest = None;
        let mut request_namespace = None;
        let mut request_stream = request.into_inner();

        while let Some(request) = request_stream.next().await {
            let request = request.map_err(|err| Status::internal(err.to_string()))?;

            request_data.extend_from_slice(&request.data);

            request_digest = Some(request.digest);
            request_namespace = Some(request.namespace);
        }

        if request_data.is_empty() {
            return Err(Status::invalid_argument("missing `data` field"));
        }

        let Some(request_digest) = request_digest else {
            return Err(Status::invalid_argument("missing `digest` field"));
        };

        let Some(request_namespace) = request_namespace else {
            return Err(Status::invalid_argument("missing `namespace` field"));
        };

        let request = ArchivePushRequest {
            digest: request_digest,
            data: request_data,
            namespace: request_namespace,
        };

        self.backend.push(&request).await?;

        info!("registry |> archive push: {}", request.digest);

        Ok(Response::new(ArchiveResponse {}))
    }
}

#[tonic::async_trait]
pub trait ArtifactBackend: Send + Sync + 'static {
    async fn get_artifact(&self, digest: String, namespace: String) -> Result<Artifact, Status>;

    async fn get_artifact_alias(
        &self,
        name: String,
        namespace: String,
        system: ArtifactSystem,
        version: String,
    ) -> Result<String, Status>;

    async fn store_artifact(
        &self,
        artifact: Artifact,
        artifact_aliases: Vec<String>,
        artifact_namespace: String,
    ) -> Result<String, Status>;

    /// Return a new `Box<dyn RegistryBackend>` cloned from `self`.
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
        // Authorization check
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_permission(&request, &req_inner.namespace, "read")?;

            if let Some(user) = get_user_context(&request) {
                info!(
                    "artifact |> get_artifact by {} in namespace {}",
                    user, req_inner.namespace
                );
            }
        }

        let request = request.into_inner();

        if request.digest.is_empty() {
            return Err(Status::invalid_argument("missing `digest` field"));
        }

        let artifact = self
            .backend
            .get_artifact(request.digest.clone(), request.namespace.clone())
            .await?;

        info!("artifact |> get: {}", request.digest);

        Ok(Response::new(artifact))
    }

    async fn get_artifact_alias(
        &self,
        request: Request<GetArtifactAliasRequest>,
    ) -> Result<Response<GetArtifactAliasResponse>, Status> {
        // Authorization check
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_permission(&request, &req_inner.namespace, "read")?;
        }

        let request = request.into_inner();

        let request_system = ArtifactSystem::try_from(request.system);

        let digest = self
            .backend
            .get_artifact_alias(
                request.name.clone(),
                request.namespace,
                request_system.unwrap_or(ArtifactSystem::UnknownSystem),
                request.tag.clone(),
            )
            .await?;

        info!(
            "artifact |> alias get: {}:{} -> {}",
            request.name, request.tag, digest
        );

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
        // Authorization check
        if request
            .extensions()
            .get::<crate::command::start::auth::Claims>()
            .is_some()
        {
            let req_inner = request.get_ref();
            require_namespace_permission(&request, &req_inner.artifact_namespace, "write")?;

            if let Some(user) = get_user_context(&request) {
                info!(
                    "artifact |> store_artifact by {} in namespace {}",
                    user, req_inner.artifact_namespace
                );
            }
        }

        let request = request.into_inner();

        let artifact = request
            .artifact
            .ok_or_else(|| Status::invalid_argument("missing `artifact` field"))?;

        let digest = self
            .backend
            .store_artifact(
                artifact,
                request.artifact_aliases,
                request.artifact_namespace,
            )
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
        ServerBackend::Local => Box::new(LocalBackend::new()?),
        ServerBackend::S3 => Box::new(
            S3Backend::new(
                registry_backend_s3_bucket.clone(),
                registry_backend_s3_force_path_style,
            )
            .await?,
        ),
        ServerBackend::Unknown => bail!("unknown archive backend: {}", registry_backend),
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
        ServerBackend::Local => Box::new(LocalBackend::new()?),
        ServerBackend::S3 => Box::new(
            S3Backend::new(
                registry_backend_s3_bucket.clone(),
                registry_backend_s3_force_path_style,
            )
            .await?,
        ),
        ServerBackend::Unknown => bail!("unknown artifact backend: {}", registry_backend),
    };

    Ok(backend_artifact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use vorpal_sdk::api::archive::archive_service_server::ArchiveService;

    /// Mock backend that tracks call counts and returns configurable results.
    struct MockBackend {
        check_call_count: Arc<AtomicUsize>,
        should_exist: bool,
    }

    impl MockBackend {
        fn new(should_exist: bool) -> Self {
            Self {
                check_call_count: Arc::new(AtomicUsize::new(0)),
                should_exist,
            }
        }

        fn call_count(&self) -> usize {
            self.check_call_count.load(Ordering::SeqCst)
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
            _tx: mpsc::Sender<Result<ArchivePullResponse, Status>>,
        ) -> Result<(), Status> {
            unimplemented!("not needed for cache tests")
        }

        async fn push(&self, _req: &ArchivePushRequest) -> Result<(), Status> {
            unimplemented!("not needed for cache tests")
        }

        fn box_clone(&self) -> Box<dyn ArchiveBackend> {
            Box::new(MockBackend {
                check_call_count: Arc::clone(&self.check_call_count),
                should_exist: self.should_exist,
            })
        }
    }

    fn make_check_request(namespace: &str, digest: &str) -> Request<ArchivePullRequest> {
        Request::new(ArchivePullRequest {
            namespace: namespace.to_string(),
            digest: digest.to_string(),
        })
    }

    #[tokio::test]
    async fn test_cache_hit_skips_backend() {
        // Given: a server with caching enabled (TTL = 300s)
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check the same archive twice
        let _ = server
            .check(make_check_request("ns", "digest1"))
            .await
            .unwrap();
        let _ = server
            .check(make_check_request("ns", "digest1"))
            .await
            .unwrap();

        // Then: backend should only be called once (second call hits cache)
        assert_eq!(backend.call_count(), 1);
    }

    #[tokio::test]
    async fn test_cache_miss_for_different_keys() {
        // Given: a server with caching enabled
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check different digests
        let _ = server
            .check(make_check_request("ns", "digest-a"))
            .await
            .unwrap();
        let _ = server
            .check(make_check_request("ns", "digest-b"))
            .await
            .unwrap();

        // Then: backend should be called twice (each is a cache miss)
        assert_eq!(backend.call_count(), 2);
    }

    #[tokio::test]
    async fn test_cache_miss_for_different_namespaces() {
        // Given: a server with caching enabled
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check the same digest in different namespaces
        let _ = server
            .check(make_check_request("ns1", "digest"))
            .await
            .unwrap();
        let _ = server
            .check(make_check_request("ns2", "digest"))
            .await
            .unwrap();

        // Then: backend should be called twice (different cache keys)
        assert_eq!(backend.call_count(), 2);
    }

    #[tokio::test]
    async fn test_negative_caching_not_found() {
        // Given: a server with a backend that returns "not found"
        let backend = MockBackend::new(false);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check the same archive twice
        let result1 = server.check(make_check_request("ns", "missing")).await;
        let result2 = server.check(make_check_request("ns", "missing")).await;

        // Then: both should return not_found
        assert!(result1.is_err());
        assert_eq!(result1.unwrap_err().code(), tonic::Code::NotFound);
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err().code(), tonic::Code::NotFound);

        // And: backend should only be called once (negative result is cached)
        assert_eq!(backend.call_count(), 1);
    }

    #[tokio::test]
    async fn test_ttl_zero_disables_caching() {
        // Given: a server with TTL = 0 (caching disabled)
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 0);

        // When: we check the same archive multiple times
        let _ = server
            .check(make_check_request("ns", "digest"))
            .await
            .unwrap();
        let _ = server
            .check(make_check_request("ns", "digest"))
            .await
            .unwrap();
        let _ = server
            .check(make_check_request("ns", "digest"))
            .await
            .unwrap();

        // Then: backend should be called every time (no caching)
        assert_eq!(backend.call_count(), 3);
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        // Given: a server with a very short TTL (1 second)
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 1);

        // When: we check, wait for TTL to expire, then check again
        let _ = server
            .check(make_check_request("ns", "digest"))
            .await
            .unwrap();
        assert_eq!(backend.call_count(), 1);

        // Wait for cache to expire
        tokio::time::sleep(Duration::from_millis(1100)).await;

        let _ = server
            .check(make_check_request("ns", "digest"))
            .await
            .unwrap();

        // Then: backend should be called twice (second call after expiration)
        assert_eq!(backend.call_count(), 2);
    }

    #[tokio::test]
    async fn test_check_returns_error_for_empty_digest() {
        // Given: a server
        let backend = MockBackend::new(true);
        let server = ArchiveServer::new(backend.box_clone(), 300);

        // When: we check with an empty digest
        let result = server.check(make_check_request("ns", "")).await;

        // Then: should return InvalidArgument error
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

        // And: backend should not be called
        assert_eq!(backend.call_count(), 0);
    }
}
