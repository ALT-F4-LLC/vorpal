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
    /// flag was omitted: see `default_registry_allowed` in this file for how
    /// it resolves — this process's own listening address when it also
    /// co-hosts the registry service, fail-closed empty otherwise.
    pub registry_allowed: Option<Vec<String>>,
    pub services: Vec<String>,
    pub tls: bool,
}

/// `ResolvedRegistry` and `resolve_registry` live in their own submodule,
/// not directly in `start`, so the type's single-constructor guarantee is
/// something the compiler actually enforces rather than a comment asserting
/// it. `pub(super)` on an item defined in `start` is visible to `start`'s
/// *parent* and, transitively, to every descendant of that parent —
/// `worker` and `agent`, both children of `start`, are such descendants, so
/// a bare tuple literal `ResolvedRegistry(attacker_str)` written in either
/// of them would have compiled. Nesting the type one module deeper closes
/// that: the tuple field below has no visibility modifier, so it is private
/// to `resolved_registry` and *its* descendants only, and `worker`/`agent`
/// are siblings of `resolved_registry`, not descendants of it (verified
/// 2026-08-24, reconcile C1: reverting this nesting while keeping the old
/// comment's claim is exactly the gap it closes).
mod resolved_registry {
    /// The registry `resolve_registry` selected. Wrapping it distinguishes
    /// it, at the type level, from `request.registry` (a bare `String` on
    /// the unvalidated request) — `worker::pull_source`,
    /// `worker::pull_artifact` and `agent::build_source` accept only this
    /// type, so a future edit that threads the raw request field into any of
    /// them instead of the resolved value fails to compile rather than
    /// silently reopening the registry-pinning check (C1/A2 in the threat
    /// model). See the module doc above for why `resolve_registry`, defined
    /// in this same module, is the compiler-enforced only constructor.
    #[derive(Debug, Clone)]
    pub(super) struct ResolvedRegistry(String);

    // Test-only: production code never compares two resolved registries or a
    // registry against a literal string, and deriving this unconditionally
    // would make it production surface (reconcile R2-C10) for a capability
    // only assertions use.
    #[cfg(test)]
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
    /// caller-supplied `requested` value and the operator-configured
    /// `allowed` set. Shared by the worker's `build_artifact` and the
    /// agent's `prepare_artifact` — both dial a caller-named registry, so
    /// both need the identical check, living here in the module both share
    /// rather than in either sibling.
    ///
    /// A request-supplied registry is at most a selector over the
    /// operator-configured set: it never introduces a registry the operator
    /// did not name. An empty `allowed` set is fail-closed — it means no
    /// registry is configured for this process, never "any registry is
    /// acceptable" (the inverse of `issuer_service_client_ids`, where empty
    /// means "trust nobody" but every token still routes through namespace
    /// RBAC rather than being refused outright).
    ///
    /// Matching is exact string equality after trimming one trailing `/`
    /// from each side — not a URI parse of scheme/host/port, and never
    /// prefix or substring, which would let
    /// `https://registry.example.com.evil.test` or a query-string trick slip
    /// past an allow-list entry of `https://registry.example.com`. Two URIs
    /// that are equivalent as parsed components (e.g. differing only in
    /// path, case, or a second trailing `/`) but differ as strings after one
    /// trim are treated as different registries — stricter than semantic URI
    /// equality, which is the safe direction for an allow-list to err in.
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

    // Moved from `worker.rs` (reconcile R2-C10): `resolve_registry` lives
    // here now, and a pure-function seam should carry its own tests rather
    // than leaving them behind in the module that merely calls it.
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn resolve_registry_defaults_to_the_sole_configured_value_when_the_request_is_silent() {
            let allowed = vec!["http://registry.example.com:9000".to_string()];

            assert_eq!(
                resolve_registry("", &allowed).unwrap(),
                "http://registry.example.com:9000"
            );
        }

        #[test]
        fn resolve_registry_refuses_a_mismatch_even_with_a_sole_configured_value() {
            let allowed = vec!["http://registry.example.com:9000".to_string()];

            let err = resolve_registry("http://attacker.example.com", &allowed).unwrap_err();

            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }

        #[test]
        fn resolve_registry_defaults_to_the_first_entry_when_the_request_is_silent() {
            let allowed = vec![
                "http://registry-a.example.com".to_string(),
                "http://registry-b.example.com".to_string(),
            ];

            assert_eq!(
                resolve_registry("", &allowed).unwrap(),
                "http://registry-a.example.com"
            );
        }

        #[test]
        fn resolve_registry_accepts_a_listed_selection() {
            let allowed = vec![
                "http://registry-a.example.com".to_string(),
                "http://registry-b.example.com".to_string(),
            ];

            assert_eq!(
                resolve_registry("http://registry-b.example.com", &allowed).unwrap(),
                "http://registry-b.example.com"
            );
        }

        #[test]
        fn resolve_registry_trims_one_trailing_slash_on_both_sides() {
            let allowed = vec!["http://registry.example.com/".to_string()];

            assert_eq!(
                resolve_registry("http://registry.example.com", &allowed).unwrap(),
                "http://registry.example.com/"
            );
        }

        // The prior test only trims the allow-list side; a request-supplied
        // trailing slash must be trimmed too, or the two sides of the same
        // normalization would be tested asymmetrically.
        #[test]
        fn resolve_registry_trims_one_trailing_slash_on_the_request_side_too() {
            let allowed = vec!["http://registry.example.com".to_string()];

            assert_eq!(
                resolve_registry("http://registry.example.com/", &allowed).unwrap(),
                "http://registry.example.com"
            );
        }

        #[test]
        fn resolve_registry_refuses_a_selection_outside_the_allow_list() {
            let allowed = vec![
                "http://registry-a.example.com".to_string(),
                "http://registry-b.example.com".to_string(),
            ];

            let err = resolve_registry("http://attacker.example.com", &allowed).unwrap_err();

            assert_eq!(err.code(), tonic::Code::InvalidArgument);
            assert!(err.message().contains("registry"), "{}", err.message());
        }

        // Prefix and substring near-misses must not be admitted by a
        // whole-URI allow-list entry: `starts_with` would let a
        // `.evil.test` suffix through, and `contains` would let a
        // query-string trick through.
        #[test]
        fn resolve_registry_refuses_prefix_and_substring_near_misses() {
            let allowed = vec!["https://registry.example.com".to_string()];

            for hostile in [
                "https://registry.example.com.evil.test",
                "https://evil.test/?u=https://registry.example.com",
            ] {
                let err = resolve_registry(hostile, &allowed).unwrap_err();

                assert_eq!(err.code(), tonic::Code::InvalidArgument, "{hostile:?}");
            }
        }

        #[test]
        fn resolve_registry_refuses_everything_when_nothing_is_configured() {
            let err = resolve_registry("http://registry.example.com", &[]).unwrap_err();

            assert_eq!(err.code(), tonic::Code::InvalidArgument);
        }
    }
}

use resolved_registry::{resolve_registry, ResolvedRegistry};

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
/// `--registry-allowed`/`VORPAL_REGISTRY_ALLOWED` is omitted and this
/// process co-hosts the registry service (see `default_registry_allowed`).
///
/// Deliberately independent of `vorpal_sdk::artifact::get_default_address()`:
/// that helper is a *client*-facing default (env `VORPAL_SOCKET_PATH`, else
/// the hardcoded Unix socket path) with no knowledge of `--port`/`--tls`, so
/// a TCP or TLS-configured process defaulted to it names a socket nothing is
/// listening on and refuses every build. This mirrors `effective_port`/
/// `args.tls` — the same values `run` already used to decide what to
/// bind — so the default always names the transport this process actually
/// serves on.
///
/// The host is `localhost`, not the `127.0.0.1` literal an earlier version
/// used (reconcile R2-C2/R2-C3): the main listener binds `[::]:<port>` — the
/// IPv6 unspecified address — and on several platforms (notably BSD/macOS,
/// where `net.inet6.ip6.v6only` defaults on) that socket does not accept an
/// IPv4 connection to `127.0.0.1` at all, so the worker's self-dial could
/// fail to even reach its own listener. `localhost` also matches the sole
/// DNS SAN the generated service certificate carries
/// (`system/keys.rs`, `let name = "localhost"`), so a `--tls` deployment's
/// self-dial can complete its handshake instead of failing hostname
/// verification against an IP literal. It is the same spelling already used
/// elsewhere in this codebase for the equivalent purpose (`command/config.rs`'s
/// `http://localhost:{command_port}`).
fn own_registry_address(effective_port: Option<u16>, tls: bool, socket_path: &Path) -> String {
    match effective_port {
        Some(port) => {
            let scheme = if tls { "https" } else { "http" };
            format!("{scheme}://localhost:{port}")
        }
        None => format!("unix://{}", socket_path.display()),
    }
}

/// The registry allow-list `run` uses when `--registry-allowed`/
/// `VORPAL_REGISTRY_ALLOWED` was not passed explicitly.
///
/// Pulled out of `run` as its own pure function, tested directly, so a
/// mutation to either wiring point it replaces — the `has_registry` gate
/// below, or the default no longer being `own_registry_address`'s result —
/// fails a test of its own instead of surviving because nothing outside
/// `run`'s untestable body exercised it (reconcile R2-C4: mutation testing
/// found exactly this gap on both the prior single-branch version here and
/// the CLI's `Start` match arm).
///
/// Defaulting to `own_registry_address` regardless of whether this process
/// runs its own registry was itself the reconcile R2-C2 bug: on a
/// worker-only (or agent-only) deployment split from its registry, "this
/// process's own address" names no registry at all, and guessing it papers
/// over a configuration the operator must actually supply. Only when this
/// process also runs the registry service (`has_registry`) is its own
/// address a meaningful default; otherwise the omitted flag falls back to
/// the same fail-closed empty list an explicit `--registry-allowed ""`
/// produces; every worker/agent registry dial refuses until the operator
/// configures one, and the startup log below still surfaces that fact.
fn default_registry_allowed(
    explicit: Option<Vec<String>>,
    has_registry: bool,
    effective_port: Option<u16>,
    tls: bool,
    socket_path: &Path,
) -> Vec<String> {
    explicit.unwrap_or_else(|| {
        if has_registry {
            vec![own_registry_address(effective_port, tls, socket_path)]
        } else {
            Vec::new()
        }
    })
}

/// VPL-434 (C1): whether starting with the given service set and `--issuer`
/// value must be refused. Worker, archive and artifact requests reach
/// `run_step`/the store with no isolation at all (`worker.rs`), so an
/// unauthenticated registration of any of them is not a narrower-permission
/// mode — it is unauthenticated arbitrary code execution as this process's
/// uid. `issuer == None` used to register those three services with no
/// interceptor (`WorkerServiceServer::new`/`ArchiveServiceServer::new`/
/// `ArtifactServiceServer::new`, no `with_interceptor`), so a peer that could
/// merely reach the listener needed no credential at all. The governing rule
/// (VPL-434 threat model §4): the absence of a credential in a request must
/// never be what authorizes it, which means "this deployment is anonymous"
/// cannot be inferred per request — it has to be refused at the one place
/// that is outside the attacker's reach, process construction. Extracted as
/// a pure predicate, mirroring `registry_allowed_log_is_active`, so this
/// decision is unit-testable without standing up a server (TDD §11).
///
/// The agent is covered by the same rule: it reads caller-named filesystem
/// paths and pushes what it reads into a caller-named registry namespace
/// under the host user's own stored OAuth credentials, so an unauthenticated
/// agent is a confused deputy for arbitrary file read and namespace forgery.
/// Refusing here is what leaves only two states — authenticated, or refused
/// to start — with no third in which the agent registers bare.
fn anonymous_start_refused(services: &StartupServices, issuer: Option<&str>) -> bool {
    (services.has_worker || services.has_registry || services.has_agent) && issuer.is_none()
}

/// Resolves the credential the worker and registry (archive/artifact)
/// services need, once, at the single point their requirement is
/// established — rather than each registration site re-deriving "an issuer
/// is present" for itself. Before this, `run` carried two `.expect()`s, 165
/// and 210 lines apart, each independently re-asserting a guarantee that
/// `anonymous_start_refused` had already checked (VPL-434-CORRECTNESS-3,
/// VPL434-ARCH-2): a bare bool told the caller *that* an issuer was
/// required, not *what* it was, so every call site had to go back to
/// `args.issuer` and re-justify pulling it out of the `Option`. Returning
/// the validated issuer itself, once, removes both `.expect()`s and the
/// bool in between.
///
/// Takes a named `StartupServices` rather than two adjacent `bool`s
/// (VPL434-ARCH-7): `resolve_required_issuer(has_registry, has_worker, ..)`
/// transposed from the intended `resolve_required_issuer(has_worker,
/// has_registry, ..)` would read identically at any call site and compile
/// either way, which is what made the swap invisible in `bool`-parameter
/// form.
struct StartupServices {
    has_worker: bool,
    has_registry: bool,
    has_agent: bool,
}

fn resolve_required_issuer(
    services: StartupServices,
    issuer: Option<String>,
) -> Result<Option<String>> {
    if anonymous_start_refused(&services, issuer.as_deref()) {
        bail!(
            "agent, worker and archive/artifact services require --issuer for authentication; \
             refusing to start unauthenticated — an anonymous peer could otherwise run \
             arbitrary build entrypoints as this process's uid, or have the agent read local \
             files and push them to the registry under this user's credentials"
        );
    }

    Ok(
        if services.has_worker || services.has_registry || services.has_agent {
            issuer
        } else {
            None
        },
    )
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
    issuer: &str,
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

    let validator_intercepter =
        new_validator_interceptor(issuer, args.issuer_audience.as_deref(), &args.issuer_service_client_ids)
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

    info!("archive |> service: {}", transport_label);
    info!("artifact |> service: {}", transport_label);

    Ok(router)
}

/// Adds the worker service to `router` when `args.services` requests it,
/// wiring in an OIDC interceptor whenever `args.issuer` is set.
async fn add_worker_service(
    mut router: tonic::transport::server::Router,
    args: &RunArgs,
    issuer: &str,
    registry_allowed: Vec<String>,
    transport_label: &str,
) -> Result<tonic::transport::server::Router> {
    // callee in start/worker.rs takes ownership; `args` is a shared reference reused below
    let worker_server = WorkerServer::new(
        Some(issuer.to_string()),
        args.issuer_audience.clone(),
        args.issuer_client_id.clone(),
        args.issuer_client_secret.clone(),
        registry_allowed,
    );

    let validator_intercepter =
        new_validator_interceptor(issuer, args.issuer_audience.as_deref(), &args.issuer_service_client_ids)
            .await?;

    router = router.add_service(WorkerServiceServer::with_interceptor(
        worker_server,
        validator_intercepter,
    ));

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
    let has_registry = args.services.contains(&"registry".to_string());

    let effective_port = resolve_effective_port(
        args.port,
        args.tls,
        args.health_check,
        args.health_check_port,
    )?;

    // VPL-434 (C1): refuse to start with an unauthenticated agent, worker or
    // registry (archive/artifact) service rather than defaulting into one.
    // See `resolve_required_issuer` for why this returns the validated
    // issuer itself rather than a bare refusal bool.
    let required_issuer = resolve_required_issuer(
        StartupServices {
            has_worker,
            has_registry,
            has_agent,
        },
        args.issuer.clone(),
    )?;

    // An omitted `--registry-allowed` resolves to this process's own
    // listening address, computed from the transport just decided above —
    // not from the client-facing `get_default_address()` helper, which
    // knows nothing about `--port`/`--tls` and would default a TCP or TLS
    // deployment to a Unix socket path nothing is listening on, refusing
    // every build — and only when this process co-hosts the registry
    // service itself; a split (worker- or agent-only) deployment has no
    // meaningful "own address" default and falls back to fail-closed empty
    // instead. An explicit (possibly empty) `--registry-allowed` is the
    // operator's own choice and is used as given. See
    // `default_registry_allowed`.
    let registry_allowed = default_registry_allowed(
        args.registry_allowed.clone(),
        has_registry,
        effective_port,
        args.tls,
        &get_socket_path(),
    );

    // Emit the registry allow-list at startup for the same reason the
    // trusted-service list is emitted above: a worker or agent with an
    // empty list refuses every registry dial (fail-closed), which should be
    // visible at boot rather than discovered from the first refused build.
    // Scoped to processes that actually run one of those two services — a
    // registry-only process configures no allow-list of its own and logging
    // about it here would be a false signal.
    if registry_allowed_log_is_active(has_worker, has_agent) {
        if registry_allowed.is_empty() {
            // Reachable either from an explicit `--registry-allowed ""` (or
            // `VORPAL_REGISTRY_ALLOWED=`, which reads identically to clap)
            // or from the omitted-flag default when this process does not
            // co-host the registry service (`default_registry_allowed`). An
            // operator who deliberately refuses every build, one running a
            // split worker-only deployment who has not yet set
            // `--registry-allowed`, and an empty env var nobody meant to set
            // are indistinguishable from a bare info line — so this is a
            // `warn!`, naming every possible source, rather than the info
            // line a healthy, fully-configured process also produces.
            warn!(
                "registry allow-list is empty (--registry-allowed \"\", \
                 VORPAL_REGISTRY_ALLOWED=\"\", or this process does not run its own \
                 registry service): every worker/agent registry dial will be refused \
                 until --registry-allowed/VORPAL_REGISTRY_ALLOWED is configured"
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

    // `required_issuer` is `Some` here exactly when `has_agent`,
    // `has_registry` or `has_worker` is true (`resolve_required_issuer`'s own
    // contract) — a `match` on it, rather than an `Option::expect()`
    // re-derived inside each `if has_registry`/`if has_worker` block below,
    // means neither block can compile against a missing issuer in the first
    // place, so there is nothing left here for
    // VPL-434-CORRECTNESS-3/VPL434-ARCH-2 to flag. Registering the agent
    // inside this arm is what makes a bare `AgentServiceServer::new`
    // unreachable: there is no branch left in which the agent is added
    // without an interceptor.
    if let Some(issuer) = required_issuer {
        if has_agent {
            let validator_intercepter = new_validator_interceptor(
                &issuer,
                args.issuer_audience.as_deref(),
                &args.issuer_service_client_ids,
            )
            .await?;

            router = router.add_service(AgentServiceServer::with_interceptor(
                AgentServer::new(registry_allowed.clone()),
                validator_intercepter,
            ));

            info!("agent |> service: {}", transport_label);
        }

        if has_registry {
            router = add_registry_services(router, &args, &issuer, &transport_label).await?;
        }

        if has_worker {
            router = add_worker_service(
                router,
                &args,
                &issuer,
                registry_allowed.clone(),
                &transport_label,
            )
            .await?;

            info!("worker |> service: {}", transport_label);
        }
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
mod anonymous_start_refused_tests {
    use super::*;

    fn services(has_worker: bool, has_registry: bool, has_agent: bool) -> StartupServices {
        StartupServices {
            has_worker,
            has_registry,
            has_agent,
        }
    }

    // AC-1/AC-2 (VPL-434): a worker or registry (archive/artifact) service
    // with no issuer must be refused, whether reachable over UDS or TCP —
    // the predicate takes no transport argument because the decision is
    // about credential absence, not reach.
    #[test]
    fn anonymous_start_refused_for_worker_with_no_issuer() {
        assert!(anonymous_start_refused(&services(true, false, false), None));
    }

    #[test]
    fn anonymous_start_refused_for_registry_with_no_issuer() {
        assert!(anonymous_start_refused(&services(false, true, false), None));
    }

    #[test]
    fn anonymous_start_not_refused_when_an_issuer_is_configured() {
        assert!(!anonymous_start_refused(
            &services(true, true, true),
            Some("https://issuer.example.com")
        ));
    }

    // An agent with no issuer used to be permitted, which registered the
    // agent service with no interceptor at all: an anonymous peer could have
    // it read local files and push them to the registry under this user's
    // stored credentials. The agent now joins worker and registry in the
    // refusal.
    #[test]
    fn anonymous_start_refused_for_agent_with_no_issuer() {
        assert!(anonymous_start_refused(&services(false, false, true), None));
    }

    // A process running none of the three has no surface for this predicate
    // to gate.
    #[test]
    fn anonymous_start_not_refused_for_a_process_running_none_of_them() {
        assert!(!anonymous_start_refused(
            &services(false, false, false),
            None
        ));
    }
}

// VPL-434-CLUSTER-6: `anonymous_start_refused` is unit-tested above, but
// nothing previously pinned that `run` actually calls it before binding a
// listener — a mutation that deleted the `if anonymous_start_refused(..) {
// bail!(..) }` block in `run` compiled and the full suite still passed
// (reconciled finding, observed: `cargo test --package vorpal-cli` 266/0
// with that block removed). Driving `run` itself, rather than the predicate
// alone, closes that gap.
#[cfg(test)]
mod run_startup_refusal_tests {
    use super::*;

    fn single_service_args(service: &str, issuer: Option<String>) -> RunArgs {
        RunArgs {
            archive_cache_ttl: 3600,
            health_check: false,
            health_check_port: 0,
            issuer,
            issuer_audience: None,
            issuer_client_id: None,
            issuer_client_secret: None,
            issuer_service_client_ids: vec![],
            port: None,
            registry_backend: "local".to_string(),
            registry_backend_s3_bucket: None,
            registry_backend_s3_force_path_style: false,
            registry_allowed: None,
            services: vec![service.to_string()],
            tls: false,
        }
    }

    // A worker service with no issuer must return `Err` from `run` without
    // ever reaching a bind — the assertion is on `run`'s own return value,
    // not on the predicate it delegates to, so a mutation that removes the
    // call site (not just the predicate) fails this test.
    #[tokio::test]
    async fn run_refuses_a_worker_service_with_no_issuer() {
        let err = run(single_service_args("worker", None)).await.unwrap_err();

        assert!(
            err.to_string().contains("require --issuer"),
            "unexpected error: {err}"
        );
    }

    // The default `--services agent` install is the deployment this refusal
    // is about: without it the agent registers with no interceptor to
    // install, so driving `run` itself — not just the predicate — is what
    // pins that a bare agent never binds a listener.
    #[tokio::test]
    async fn run_refuses_an_agent_service_with_no_issuer() {
        let err = run(single_service_args("agent", None)).await.unwrap_err();

        assert!(
            err.to_string().contains("require --issuer"),
            "unexpected error: {err}"
        );
    }
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

    // R2-C2/R2-C3 (reconcile): `localhost`, not the `127.0.0.1` literal —
    // the listener binds the IPv6 unspecified address, which does not accept
    // `127.0.0.1` connections on every platform, and the generated service
    // certificate's sole DNS SAN is `localhost`, not an IP literal.
    #[test]
    fn own_registry_address_uses_http_when_a_plaintext_port_is_bound() {
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");
        assert_eq!(
            own_registry_address(Some(23151), false, socket_path),
            "http://localhost:23151"
        );
    }

    #[test]
    fn own_registry_address_uses_https_when_tls_is_enabled() {
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");
        assert_eq!(
            own_registry_address(Some(23151), true, socket_path),
            "https://localhost:23151"
        );
    }
}

#[cfg(test)]
mod default_registry_allowed_tests {
    use super::*;

    // R2-C4 (reconcile): pin the wiring `run` uses, not just the pieces it
    // calls — reverting either the `has_registry` gate or the default back
    // to `own_registry_address` unconditionally fails one of these.
    #[test]
    fn default_registry_allowed_uses_explicit_value_regardless_of_has_registry() {
        let explicit = vec!["http://explicit.example.com".to_string()];
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");

        assert_eq!(
            default_registry_allowed(
                Some(explicit.clone()),
                true,
                Some(23151),
                false,
                socket_path
            ),
            explicit
        );
        assert_eq!(
            default_registry_allowed(Some(explicit.clone()), false, None, false, socket_path),
            explicit
        );
    }

    // R2-C2: a split deployment (this process runs no registry of its own)
    // gets no guessed default — fail-closed empty, same as an explicit
    // `--registry-allowed ""`.
    #[test]
    fn default_registry_allowed_is_empty_when_this_process_has_no_registry() {
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");

        assert_eq!(
            default_registry_allowed(None, false, Some(23151), false, socket_path),
            Vec::<String>::new()
        );
    }

    #[test]
    fn default_registry_allowed_uses_own_address_when_this_process_has_a_registry() {
        let socket_path = Path::new("/var/lib/vorpal/vorpal.sock");

        assert_eq!(
            default_registry_allowed(None, true, Some(23151), false, socket_path),
            vec!["http://localhost:23151".to_string()]
        );
    }
}
