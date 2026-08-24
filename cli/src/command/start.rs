use crate::command::{
    start::{
        agent::AgentServer,
        registry::{
            backend_archive, backend_artifact, ArchiveServer, ArtifactServer, ServerBackend,
        },
        worker::WorkerServer,
    },
    store::paths::{
        get_key_service_key_path, get_key_service_path, get_lock_path, get_socket_path,
    },
};
use anyhow::{bail, Result};
use fs4::fs_std::FileExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::read_to_string;
use tokio::net::{TcpListener, UnixListener};
use tokio_stream::wrappers::{TcpListenerStream, UnixListenerStream};
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic_health::{pb::health_server::HealthServer, server::HealthService};
use tracing::{info, warn};
use vorpal_sdk::api::{
    agent::agent_service_server::AgentServiceServer,
    archive::archive_service_server::ArchiveServiceServer,
    artifact::artifact_service_server::ArtifactServiceServer,
    worker::worker_service_server::WorkerServiceServer,
};

mod agent;
pub mod auth;
mod registry;
mod worker;

pub struct RunArgs {
    pub archive_cache_ttl: u64,
    pub health_check: bool,
    pub health_check_port: u16,
    pub issuer: Option<String>,
    pub issuer_audience: Option<String>,
    pub issuer_client_id: Option<String>,
    pub issuer_client_secret: Option<String>,
    /// OAuth client IDs whose tokens classify as `PrincipalKind::TrustedService`
    /// and bypass namespace RBAC. Populated from `--issuer-service-client-ids`
    /// (or `VORPAL_ISSUER_SERVICE_CLIENT_IDS`) in `cli/src/command.rs`. Empty
    /// by default — every token then follows the `Human` path (current
    /// behavior, TDD §6 "Backward compatibility").
    pub issuer_service_client_ids: Vec<String>,
    pub port: Option<u16>,
    pub registry_backend: String,
    pub registry_backend_s3_bucket: Option<String>,
    pub registry_backend_s3_force_path_style: bool,
    /// Registries the worker's `build_artifact` may pull from or push to. A
    /// request-supplied `registry` is at most a selector over this set; an
    /// explicit empty set means no registry is configured (fail-closed —
    /// never "any registry"). Populated from `--registry-allowed` (or
    /// `VORPAL_REGISTRY_ALLOWED`) in `cli/src/command.rs`. `None` means the
    /// flag was omitted: `run` below resolves it to this process's own
    /// listening address — computed from `effective_port`/`tls`, not from
    /// the client-facing `get_default_address()` helper, so a TCP or TLS
    /// deployment (or one where the registry runs split from this worker's
    /// socket default) gets a default that actually matches what it is
    /// listening on rather than refusing every build.
    pub registry_allowed: Option<Vec<String>>,
    pub services: Vec<String>,
    pub tls: bool,
}

/// The registry `resolve_registry` selected. Wrapping it distinguishes it,
/// at the type level, from `request.registry` (a bare `String` on the
/// unvalidated request) — `worker::pull_source`, `worker::pull_artifact` and
/// `agent::build_source` accept only this type, so a future edit that
/// threads the raw request field into any of them instead of the resolved
/// value fails to compile rather than silently reopening the
/// registry-pinning check (C1/A2 in the threat model). The tuple field has
/// no visibility modifier, so it is private to this module: neither `worker`
/// nor `agent` (both children of this module) can construct one with a bare
/// tuple literal — `resolve_registry`, defined here, is structurally the
/// only constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedRegistry(String);

impl PartialEq<&str> for ResolvedRegistry {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl std::ops::Deref for ResolvedRegistry {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ResolvedRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Resolves the registry a build may pull from and push to, from the
/// caller-supplied `requested` value and the operator-configured `allowed`
/// set. Shared by the worker's `build_artifact` and the agent's
/// `prepare_artifact` — both dial a caller-named registry, so both need the
/// identical check, living here in the module both share rather than in
/// either sibling.
///
/// A request-supplied registry is at most a selector over the
/// operator-configured set: it never introduces a registry the operator did
/// not name. An empty `allowed` set is fail-closed — it means no registry is
/// configured for this process, never "any registry is acceptable" (the
/// inverse of `issuer_service_client_ids`, where empty means "trust nobody"
/// but every token still routes through namespace RBAC rather than being
/// refused outright).
///
/// Matching is exact string equality after trimming one trailing `/` from
/// each side — not a URI parse of scheme/host/port, and never prefix or
/// substring, which would let `https://registry.example.com.evil.test` or a
/// query-string trick slip past an allow-list entry of
/// `https://registry.example.com`. Two URIs that are equivalent as parsed
/// components (e.g. differing only in path, case, or a second trailing `/`)
/// but differ as strings after one trim are treated as different registries
/// — stricter than semantic URI equality, which is the safe direction for an
/// allow-list to err in.
pub(super) fn resolve_registry(
    requested: &str,
    allowed: &[String],
) -> Result<ResolvedRegistry, tonic::Status> {
    fn normalized(value: &str) -> &str {
        value.strip_suffix('/').unwrap_or(value)
    }

    let Some(default_registry) = allowed.first() else {
        return Err(tonic::Status::invalid_argument(
            "no registry is configured for this worker",
        ));
    };

    if requested.is_empty() {
        return Ok(ResolvedRegistry(default_registry.clone()));
    }

    allowed
        .iter()
        .find(|candidate| normalized(candidate) == normalized(requested))
        .cloned()
        .map(ResolvedRegistry)
        .ok_or_else(|| {
            tonic::Status::invalid_argument(format!(
                "registry {requested:?} is not in the configured allow-list"
            ))
        })
}

/// `build_channel` (sdk/rust/src/context.rs) skips TLS entirely for the
/// `http://` scheme, so an allow-list entry using it carries the worker's
/// service bearer token in cleartext. `unix://` is excluded deliberately: it
/// is a same-host socket, so the token never crosses a network wire, and
/// warning on it would tell the operator to act on a channel that carries no
/// exposure.
fn registry_crosses_network_without_tls(entry: &str) -> bool {
    entry.starts_with("http://")
}

/// This process's own registry endpoint, in the shape `resolve_registry`
/// compares against — used as the sole default allow-list entry when
/// `--registry-allowed`/`VORPAL_REGISTRY_ALLOWED` is omitted.
///
/// Deliberately independent of `vorpal_sdk::artifact::get_default_address()`:
/// that helper is a *client*-facing default (env `VORPAL_SOCKET_PATH`, else
/// the hardcoded Unix socket path) with no knowledge of `--port`/`--tls`, so
/// a TCP or TLS-configured process defaulted to it names a socket nothing is
/// listening on and refuses every build. This mirrors `effective_port`/
/// `args.tls` — the same values `run` already used to decide what to
/// bind — so the default always names the transport this process actually
/// serves on.
fn own_registry_address(effective_port: Option<u16>, tls: bool, socket_path: &Path) -> String {
    match effective_port {
        Some(port) => {
            let scheme = if tls { "https" } else { "http" };
            format!("{scheme}://127.0.0.1:{port}")
        }
        None => format!("unix://{}", socket_path.display()),
    }
}

/// Whether `run` should emit the registry allow-list startup log at all —
/// scoped to processes running a worker or an agent, since a registry-only
/// process configures no allow-list of its own and logging about it would
/// be a false signal. The gate itself, not just the predicate the log body
/// calls, is what a mis-wired `services` list could silently skip.
fn registry_allowed_log_is_active(has_worker: bool, has_agent: bool) -> bool {
    has_worker || has_agent
}

/// The entries of `registry_allowed` that carry the worker's service bearer
/// token in cleartext — the cleartext-scheme warning loop, pulled out as
/// pure logic so it is testable without standing up a server.
fn registries_needing_tls_warning(registry_allowed: &[String]) -> Vec<&str> {
    registry_allowed
        .iter()
        .filter(|entry| registry_crosses_network_without_tls(entry))
        .map(String::as_str)
        .collect()
}

async fn new_tls_config() -> Result<ServerTlsConfig> {
    let cert_path = get_key_service_path();

    if !cert_path.exists() {
        return Err(anyhow::anyhow!(
            "public key not found - run 'vorpal system keys generate' or copy from agent"
        ));
    }

    let private_key_path = get_key_service_key_path();

    if !private_key_path.exists() {
        return Err(anyhow::anyhow!(
            "private key not found - run 'vorpal system keys generate' or copy from agent"
        ));
    }

    let cert = read_to_string(&cert_path).await.map_err(|err| {
        anyhow::anyhow!("failed to read public key {}: {}", cert_path.display(), err)
    })?;

    let private_key = read_to_string(&private_key_path).await.map_err(|err| {
        anyhow::anyhow!(
            "failed to read private key {}: {}",
            private_key_path.display(),
            err
        )
    })?;

    let config_identity = Identity::from_pem(cert, private_key);

    let config = ServerTlsConfig::new().identity(config_identity);

    Ok(config)
}

/// Builds an `OidcValidator` for `issuer`/`args.issuer_audience`, wrapped as
/// an interceptor. Each caller constructs its own validator instance (a
/// deliberate choice: sharing one validator across services would share one
/// JWKS cache between them, which is outside the scope of this refactor).
async fn new_validator_interceptor(
    issuer: &str,
    issuer_audience: Option<&str>,
    issuer_service_client_ids: &[String],
) -> Result<impl Fn(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> + Clone> {
    let mut validator_audiences = vec![];

    if let Some(audience) = issuer_audience {
        validator_audiences.push(audience.to_string());
    }

    let validator = Arc::new(
        auth::OidcValidator::new(issuer.to_string(), validator_audiences)
            .await?
            .with_trusted_service_client_ids(issuer_service_client_ids.to_vec()),
    );

    Ok(auth::new_interceptor(validator))
}

/// Adds the archive/artifact (registry) services to `router` when
/// `args.services` requests them, wiring in an OIDC interceptor whenever
/// `args.issuer` is set.
async fn add_registry_services(
    mut router: tonic::transport::server::Router,
    args: &RunArgs,
    transport_label: &str,
) -> Result<tonic::transport::server::Router> {
    let backend = match args.registry_backend.as_str() {
        "local" => ServerBackend::Local,
        "s3" => ServerBackend::S3,
        _ => ServerBackend::Unknown,
    };

    if backend == ServerBackend::Unknown {
        bail!("unknown registry backend: {}", args.registry_backend);
    }

    if backend == ServerBackend::S3 && args.registry_backend_s3_bucket.is_none() {
        bail!("s3 backend requires '--registry-backend-s3-bucket' parameter");
    }

    // callees in start/registry.rs take ownership; kept for now
    let backend_archive = backend_archive(
        args.registry_backend.clone(),
        args.registry_backend_s3_bucket.clone(),
        args.registry_backend_s3_force_path_style,
    )
    .await?;

    // callee in start/registry.rs takes ownership; kept for now
    let backend_artifact = backend_artifact(
        &args.registry_backend,
        args.registry_backend_s3_bucket.clone(),
        args.registry_backend_s3_force_path_style,
    )
    .await?;

    let archive_server = ArchiveServer::new(backend_archive, args.archive_cache_ttl);
    let artifact_server = ArtifactServer::new(backend_artifact);

    if let Some(issuer) = &args.issuer {
        let validator_intercepter = new_validator_interceptor(
            issuer,
            args.issuer_audience.as_deref(),
            &args.issuer_service_client_ids,
        )
        .await?;

        router = router.add_service(ArchiveServiceServer::with_interceptor(
            archive_server,
            // shared with the artifact service registered just below
            validator_intercepter.clone(),
        ));

        router = router.add_service(ArtifactServiceServer::with_interceptor(
            artifact_server,
            validator_intercepter,
        ));
    } else {
        router = router.add_service(ArchiveServiceServer::new(archive_server));
        router = router.add_service(ArtifactServiceServer::new(artifact_server));
    }

    info!("archive |> service: {}", transport_label);
    info!("artifact |> service: {}", transport_label);

    Ok(router)
}

/// Adds the worker service to `router` when `args.services` requests it,
/// wiring in an OIDC interceptor whenever `args.issuer` is set.
async fn add_worker_service(
    mut router: tonic::transport::server::Router,
    args: &RunArgs,
    registry_allowed: Vec<String>,
    transport_label: &str,
) -> Result<tonic::transport::server::Router> {
    // callee in start/worker.rs takes ownership; `args` is a shared reference reused below
    let worker_server = WorkerServer::new(
        args.issuer.clone(),
        args.issuer_audience.clone(),
        args.issuer_client_id.clone(),
        args.issuer_client_secret.clone(),
        registry_allowed,
    );

    if let Some(issuer) = &args.issuer {
        let validator_intercepter = new_validator_interceptor(
            issuer,
            args.issuer_audience.as_deref(),
            &args.issuer_service_client_ids,
        )
        .await?;

        router = router.add_service(WorkerServiceServer::with_interceptor(
            worker_server,
            validator_intercepter,
        ));
    } else {
        router = router.add_service(WorkerServiceServer::new(worker_server));
    }

    info!("worker |> service: {}", transport_label);

    Ok(router)
}

/// If `socket_path` exists, checks whether it is still backed by a live
/// listener and removes it when it is merely a stale leftover. Errors if the
/// socket is live (already in use) or the liveness check itself fails.
async fn reap_stale_socket(socket_path: &std::path::Path) -> Result<()> {
    if !socket_path.exists() {
        return Ok(());
    }

    match tokio::net::UnixStream::connect(socket_path).await {
        Ok(_) => {
            bail!(
                "socket {} is already in use by a running instance",
                socket_path.display()
            );
        }
        Err(e)
            if e.kind() == std::io::ErrorKind::ConnectionRefused
                || e.kind() == std::io::ErrorKind::NotConnected =>
        {
            info!(
                "removing stale socket file ({}): {}",
                e.kind(),
                socket_path.display()
            );
            tokio::fs::remove_file(socket_path).await.map_err(|err| {
                anyhow::anyhow!(
                    "failed to remove stale socket {}: {}",
                    socket_path.display(),
                    err
                )
            })?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            bail!(
                "socket {} exists but permission denied — it may belong to another user",
                socket_path.display()
            );
        }
        Err(e) => {
            bail!(
                "failed to check existing socket {}: {}",
                socket_path.display(),
                e
            );
        }
    }

    Ok(())
}

/// Serves `router` over a Unix domain socket, guarding against a
/// concurrently running instance with an advisory lock file plus a
/// stale-socket liveness check, and removing the socket file on shutdown.
async fn serve_uds(
    router: tonic::transport::server::Router,
    sigterm: &mut tokio::signal::unix::Signal,
) -> Result<()> {
    let socket_path = get_socket_path();

    // Ensure parent directories exist (shared by socket and lock file)
    let lock_path = get_lock_path();
    if let Some(parent) = lock_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|err| {
            anyhow::anyhow!(
                "failed to create lock directory {}: {}",
                parent.display(),
                err
            )
        })?;
    }

    // Acquire advisory lock to prevent TOCTOU races with stale socket detection.
    // The lock is held for the lifetime of this function (released on drop
    // when `lock_file` goes out of scope at the end).
    let lock_file = std::fs::File::create(&lock_path).map_err(|err| {
        anyhow::anyhow!(
            "failed to create lock file {}: {}",
            lock_path.display(),
            err
        )
    })?;
    let acquired = lock_file.try_lock_exclusive().map_err(|err| {
        anyhow::anyhow!("failed to acquire lock on {}: {}", lock_path.display(), err)
    })?;
    if !acquired {
        bail!(
            "another instance is already running (lock file: {})",
            lock_path.display()
        );
    }

    reap_stale_socket(&socket_path).await?;

    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|err| {
            anyhow::anyhow!(
                "failed to create socket directory {}: {}",
                parent.display(),
                err
            )
        })?;
    }
    let uds_listener = UnixListener::bind(&socket_path).map_err(|err| {
        anyhow::anyhow!(
            "failed to bind unix socket {}: {}",
            socket_path.display(),
            err
        )
    })?;
    tokio::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o660))
        .await
        .map_err(|err| {
            anyhow::anyhow!(
                "failed to set socket permissions on {}: {}",
                socket_path.display(),
                err
            )
        })?;
    info!("listening on unix socket: {}", socket_path.display());
    let main_incoming = UnixListenerStream::new(uds_listener);
    let result = serve_with_shutdown(router.serve_with_incoming(main_incoming), sigterm).await;
    // Clean up socket file on shutdown (covers both clean exit and signal).
    // The lock file is intentionally left on disk — the advisory lock is
    // released automatically when `lock_file` is dropped at the end of this
    // function. Deleting it here would race with a new instance creating a
    // fresh inode and acquiring its own lock.
    if let Err(err) = tokio::fs::remove_file(&socket_path).await {
        warn!("failed to remove socket file on shutdown: {}", err);
    }
    result
}

/// Binds the plaintext-TCP health-check listener and its own single-service
/// router, when `args.health_check` is set. Returns the router, listener, and
/// its bound address for the caller to spawn once the main server is ready.
async fn prepare_health_check(
    args: &RunArgs,
    health_reporter: &tonic_health::server::HealthReporter,
) -> Result<Option<(tonic::transport::server::Router, TcpListener, String)>> {
    if !args.health_check {
        return Ok(None);
    }

    // `HealthReporter` is a shared handle; caller keeps its own for the readiness task
    let health_service_plaintext =
        HealthServer::new(HealthService::from_health_reporter(health_reporter.clone()));

    let health_address = format!("[::]:{}", args.health_check_port);

    let health_listener = TcpListener::bind(&health_address).await.map_err(|err| {
        anyhow::anyhow!("failed to bind health server on {health_address}: {err}")
    })?;

    let health_router = Server::builder().add_service(health_service_plaintext);

    Ok(Some((health_router, health_listener, health_address)))
}

/// Logs the trusted-service-client allow-list at startup so operators (and
/// on-call) can confirm which OAuth client IDs bypass namespace RBAC.
/// Mis-wiring at construction time (forgetting to thread the list into one of
/// the validators) would otherwise fail silently — this log is the TDD §11
/// "startup log line" signal that catches it.
fn log_trusted_service_clients(issuer_service_client_ids: &[String]) {
    if issuer_service_client_ids.is_empty() {
        info!("no trusted service clients configured");
    } else {
        info!(
            "trusted service clients configured ({}): {}",
            issuer_service_client_ids.len(),
            issuer_service_client_ids.join(", ")
        );
    }
}

/// Determines the effective listen port (TLS implies TCP on 23151 by
/// default; an explicit `--port` also uses TCP; otherwise UDS), and rejects
/// a health-check port that collides with it.
fn resolve_effective_port(
    port: Option<u16>,
    tls: bool,
    health_check: bool,
    health_check_port: u16,
) -> Result<Option<u16>> {
    let effective_port = match (port, tls) {
        (Some(port), _) => Some(port),
        (None, true) => Some(23151),
        (None, false) => None, // UDS mode
    };

    if let Some(port) = effective_port {
        if health_check && health_check_port == port {
            bail!(
                "health check port ({health_check_port}) must differ from the main service port ({port})"
            );
        }
    }

    Ok(effective_port)
}

async fn serve_with_shutdown(
    serve_future: impl std::future::Future<Output = Result<(), tonic::transport::Error>>,
    sigterm: &mut tokio::signal::unix::Signal,
) -> Result<()> {
    tokio::select! {
        res = serve_future => {
            res.map_err(|err| anyhow::anyhow!("main server failed: {err}"))
        }
        _ = tokio::signal::ctrl_c() => {
            info!("received SIGINT, shutting down");
            Ok(())
        }
        _ = sigterm.recv() => {
            info!("received SIGTERM, shutting down");
            Ok(())
        }
    }
}

pub async fn run(args: RunArgs) -> Result<()> {
    log_trusted_service_clients(&args.issuer_service_client_ids);

    let has_worker = args.services.contains(&"worker".to_string());
    let has_agent = args.services.contains(&"agent".to_string());

    let effective_port = resolve_effective_port(
        args.port,
        args.tls,
        args.health_check,
        args.health_check_port,
    )?;

    // An omitted `--registry-allowed` resolves to this process's own
    // listening address, computed from the transport just decided above —
    // not from the client-facing `get_default_address()` helper, which
    // knows nothing about `--port`/`--tls` and would default a TCP or TLS
    // deployment to a Unix socket path nothing is listening on, refusing
    // every build. An explicit (possibly empty) `--registry-allowed` is the
    // operator's own choice and is used as given.
    let registry_allowed = args.registry_allowed.clone().unwrap_or_else(|| {
        vec![own_registry_address(effective_port, args.tls, &get_socket_path())]
    });

    // Emit the registry allow-list at startup for the same reason the
    // trusted-service list is emitted above: a worker or agent with an
    // empty list refuses every registry dial (fail-closed), which should be
    // visible at boot rather than discovered from the first refused build.
    // Scoped to processes that actually run one of those two services — a
    // registry-only process configures no allow-list of its own and logging
    // about it here would be a false signal.
    if registry_allowed_log_is_active(has_worker, has_agent) {
        if registry_allowed.is_empty() {
            // Only reachable when `--registry-allowed ""` (or
            // `VORPAL_REGISTRY_ALLOWED=`, which reads identically to clap)
            // was passed explicitly — the omitted-flag case is defaulted to
            // `own_registry_address` above and is never empty. An operator
            // deliberately refusing every build on this process is
            // indistinguishable, from a bare info line, from an empty env
            // var nobody meant to set — so this is a `warn!`, naming both
            // possible sources, rather than the info line a healthy,
            // fully-configured process also produces.
            warn!(
                "registry allow-list is explicitly empty (--registry-allowed \"\" or \
                 VORPAL_REGISTRY_ALLOWED=\"\"); every worker/agent registry dial will \
                 be refused"
            );
        } else {
            info!(
                "registry allow-list configured ({}): {}",
                registry_allowed.len(),
                registry_allowed.join(", ")
            );

            // `http://` channels carry no TLS (`build_channel`,
            // sdk/rust/src/context.rs), so the worker's service bearer
            // token — attached to every RPC on that channel — crosses in
            // cleartext. The allow-list has no scheme policy to refuse this
            // outright, so this is a warning an operator can act on, not a
            // refusal. `unix://` is excluded: see
            // `registry_crosses_network_without_tls`.
            for entry in registries_needing_tls_warning(&registry_allowed) {
                warn!(
                    "registry allow-list entry {entry:?} carries no TLS; \
                     service bearer tokens cross it in cleartext"
                );
            }
        }
    }

    let (health_reporter, health_service) = tonic_health::server::health_reporter();

    let mut router = if args.tls {
        info!("TLS enabled for main listener");
        let tls_config = new_tls_config().await?;
        Server::builder()
            .tls_config(tls_config)?
            .add_service(health_service)
    } else {
        let transport = if effective_port.is_some() {
            "plaintext TCP"
        } else {
            "Unix domain socket"
        };
        info!("TLS disabled, using {} for main listener", transport);
        Server::builder().add_service(health_service)
    };

    let health_prepared = prepare_health_check(&args, &health_reporter).await?;

    let transport_label = match effective_port {
        Some(port) => format!("[::]:{port}"),
        None => get_socket_path().display().to_string(),
    };

    if has_agent {
        let service = AgentServiceServer::new(AgentServer::new(registry_allowed.clone()));

        router = router.add_service(service);

        info!("agent |> service: {}", transport_label);
    }

    let has_registry = args.services.contains(&"registry".to_string());

    if has_registry {
        router = add_registry_services(router, &args, &transport_label).await?;
    }

    if has_worker {
        router =
            add_worker_service(router, &args, registry_allowed.clone(), &transport_label).await?;

        info!("worker |> service: {}", transport_label);
    }

    tokio::spawn(async move {
        tokio::task::yield_now().await;

        if has_agent {
            health_reporter
                .set_serving::<AgentServiceServer<AgentServer>>()
                .await;
        }

        if has_registry {
            health_reporter
                .set_serving::<ArchiveServiceServer<ArchiveServer>>()
                .await;
            health_reporter
                .set_serving::<ArtifactServiceServer<ArtifactServer>>()
                .await;
        }

        if has_worker {
            health_reporter
                .set_serving::<WorkerServiceServer<WorkerServer>>()
                .await;
        }
    });

    let health_handle =
        if let Some((health_router, health_listener, health_address)) = health_prepared {
            let health_incoming = TcpListenerStream::new(health_listener);

            if effective_port.is_none() {
                info!(
                    "health check using TCP (port {}) while main services use UDS",
                    args.health_check_port
                );
            }

            info!("health |> service: {}", health_address);

            Some(tokio::spawn(async move {
                if let Err(err) = health_router.serve_with_incoming(health_incoming).await {
                    warn!("health server failed: {}", err);
                }
            }))
        } else {
            None
        };

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|err| anyhow::anyhow!("failed to register SIGTERM handler: {err}"))?;

    // Bind and serve: UDS when no port specified, TCP otherwise
    let result = if let Some(port) = effective_port {
        let address = format!("[::]:{port}");
        let main_listener = TcpListener::bind(&address)
            .await
            .map_err(|err| anyhow::anyhow!("failed to bind main server on {address}: {err}"))?;
        info!("listening on TCP: {}", address);
        let main_incoming = TcpListenerStream::new(main_listener);
        serve_with_shutdown(router.serve_with_incoming(main_incoming), &mut sigterm).await
    } else {
        serve_uds(router, &mut sigterm).await
    };

    if let Some(handle) = health_handle {
        handle.abort();
    }

    result
}

#[cfg(test)]
mod registry_scheme_tests {
    use super::*;

    #[test]
    fn registry_crosses_network_without_tls_for_http() {
        assert!(registry_crosses_network_without_tls(
            "http://registry.example.com"
        ));
    }

    // C3 (reconcile): a unix:// entry is a same-host socket and must not
    // trigger the cleartext-over-network warning.
    #[test]
    fn registry_crosses_network_without_tls_is_false_for_unix() {
        assert!(!registry_crosses_network_without_tls(
            "unix:///var/lib/vorpal/vorpal.sock"
        ));
    }

    #[test]
    fn registry_crosses_network_without_tls_is_false_for_https() {
        assert!(!registry_crosses_network_without_tls(
            "https://registry.example.com"
        ));
    }

    // C11 (reconcile): pin the cleartext-warning loop itself, not just the
    // predicate it calls.
    #[test]
    fn registries_needing_tls_warning_filters_to_only_http_entries() {
        let allowed = vec![
            "http://registry.example.com".to_string(),
            "https://registry.example.com".to_string(),
            "unix:///var/lib/vorpal/vorpal.sock".to_string(),
        ];

        assert_eq!(
            registries_needing_tls_warning(&allowed),
            vec!["http://registry.example.com"]
        );
    }

    #[test]
    fn registries_needing_tls_warning_is_empty_when_nothing_qualifies() {
        let allowed = vec![
            "https://registry.example.com".to_string(),
            "unix:///var/lib/vorpal/vorpal.sock".to_string(),
        ];

        assert!(registries_needing_tls_warning(&allowed).is_empty());
    }

    // C11 (reconcile): pin the startup-log gate itself, not just the
    // predicate the log body calls.
    #[test]
    fn registry_allowed_log_is_active_for_worker_or_agent() {
        assert!(registry_allowed_log_is_active(true, false));
        assert!(registry_allowed_log_is_active(false, true));
        assert!(registry_allowed_log_is_active(true, true));
    }

    #[test]
    fn registry_allowed_log_is_inactive_for_registry_only_process() {
        assert!(!registry_allowed_log_is_active(false, false));
    }
}

#[cfg(test)]
mod own_registry_address_tests {
    use super::*;

    // C1 (reconcile): the default must reflect this process's own listening
    // transport, not the client-facing `get_default_address()` helper.
    #[test]
    fn own_registry_address_uses_unix_socket_in_uds_mode() {
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");
        assert_eq!(
            own_registry_address(None, false, socket_path),
            "unix:///var/lib/vorpal/vorpal.sock"
        );
    }

    #[test]
    fn own_registry_address_uses_http_when_a_plaintext_port_is_bound() {
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");
        assert_eq!(
            own_registry_address(Some(23151), false, socket_path),
            "http://127.0.0.1:23151"
        );
    }

    #[test]
    fn own_registry_address_uses_https_when_tls_is_enabled() {
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");
        assert_eq!(
            own_registry_address(Some(23151), true, socket_path),
            "https://127.0.0.1:23151"
        );
    }
}
