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
use anyhow::{anyhow, bail, Result};
use fs4::fs_std::FileExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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
    /// Maximum concurrent `build_artifact` executions on the worker, already
    /// floored to `1`. Populated from `--worker-jobs` (or
    /// `VORPAL_WORKER_JOBS`) in `cli/src/command.rs`.
    pub worker_jobs: usize,
    /// The directory a caller-supplied `artifact_context` must resolve inside
    /// before the agent reads, walks or writes it. Populated from
    /// `--workspace-root` (or `VORPAL_WORKSPACE_ROOT`) in
    /// `cli/src/command.rs`. `None` means the flag was omitted and the root
    /// defaults to this process's own working directory — see
    /// `resolve_workspace_root`.
    pub workspace_root: Option<PathBuf>,
}

/// Resolves the agent's workspace root once at startup, in the canonical form
/// `agent::resolve_artifact_context` compares against — an uncanonicalized
/// root would refuse every request on any host where the configured path
/// traverses a symlink (`/var` -> `/private/var` on macOS).
///
/// An omitted flag defaults to this process's working directory, which is only
/// as confining as wherever the operator started the agent: under a service
/// manager that is often `/`, and a root of `/` admits every path. The caller
/// logs the effective root and whether it came from the flag so that
/// configuration is visible at boot rather than inferred from what a build
/// managed to read.
///
/// A configured root that does not resolve is a startup error, not a silent
/// fallback: an agent that cannot establish its confinement boundary must not
/// come up serving requests without one.
fn resolve_workspace_root(configured: Option<PathBuf>) -> Result<PathBuf> {
    let root = match configured {
        Some(root) => root,
        None => std::env::current_dir().map_err(|e| {
            anyhow!("failed to read the current directory for the workspace root: {e}")
        })?,
    };

    root.canonicalize().map_err(|e| {
        anyhow!(
            "workspace root {} could not be resolved: {e}",
            root.display()
        )
    })
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
    #[expect(
        clippy::unwrap_used,
        reason = "test assertions read as intent, not defensive code: an unwrap failure is the test failing, which is the point"
    )]
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

/// VPL-434 (C1): whether starting with the given service set, `--issuer`
/// value and main listener must be refused. Worker, archive and artifact
/// requests reach `run_step`/the store with no isolation at all
/// (`worker.rs`), and the agent reads caller-named paths and pushes them
/// under the host user's stored credentials, so serving any of them to a
/// caller with no identity is unauthenticated code execution as this
/// process's uid. The governing rule (VPL-434 threat model §4): the absence
/// of a credential in a request must never be what authorizes it, so the
/// decision is made here, at process construction, never per request.
///
/// Without an issuer the services start only on the unix socket, where
/// `auth::LocalPeerInterceptor` identifies every caller by its
/// kernel-reported uid. A TCP or TLS listener has no such identity, so an
/// issuer-less start there is refused. `main_port` is the value
/// `resolve_effective_port` returns and `run` binds, so the decision cannot
/// describe a different listener from the one that serves.
fn anonymous_start_refused(
    services: StartupServices,
    issuer: Option<&str>,
    main_port: Option<u16>,
) -> bool {
    services.requires_authentication() && issuer.is_none() && main_port.is_some()
}

/// Whether a `--services` list, split into `RunArgs::services`, starts a
/// service that must authenticate its callers: the worker, the registry, or
/// the agent. The startup log reads this so it describes the same mode `run`
/// resolves.
pub(crate) fn requires_authentication(services: &[String]) -> bool {
    StartupServices::from_services(services).requires_authentication()
}

/// Whether `run` refuses a start with this `--services` list, `--port` and
/// `--tls` when no `--issuer` is configured. The startup log reads this so a
/// refusal it announces is the one `run` enforces.
pub(crate) fn issuerless_start_refused(services: &[String], port: Option<u16>, tls: bool) -> bool {
    anonymous_start_refused(
        StartupServices::from_services(services),
        None,
        main_listener_port(port, tls),
    )
}

/// Takes a named `StartupServices` rather than adjacent `bool`s
/// (VPL434-ARCH-7): a transposed `(has_registry, has_worker, ..)` would read
/// identically at any call site and compile either way.
#[derive(Clone, Copy)]
struct StartupServices {
    has_worker: bool,
    has_registry: bool,
    has_agent: bool,
}

impl StartupServices {
    /// The projection from the operator's `--services` list to the three
    /// flags every downstream decision reads. It lives here, taking
    /// `RunArgs`, rather than as three `contains` calls inline in `run`, so
    /// the enumeration test in `registration_enumeration_tests` can start
    /// from a real `RunArgs` — the thing VPL-713's AC-2 names — instead of
    /// from flags a test set by hand, and so a typo in one of the three
    /// service-name literals is a test failure rather than a service that
    /// silently never registers.
    fn from_run_args(args: &RunArgs) -> Self {
        Self::from_services(&args.services)
    }

    fn from_services(services: &[String]) -> Self {
        Self {
            has_worker: services.iter().any(|service| service == "worker"),
            has_registry: services.iter().any(|service| service == "registry"),
            has_agent: services.iter().any(|service| service == "agent"),
        }
    }

    fn requires_authentication(self) -> bool {
        self.has_worker || self.has_registry || self.has_agent
    }
}

/// How agent, registry and worker callers are identified, decided once at
/// process construction.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ServiceAuthMode {
    /// Every request carries a bearer token validated against this issuer.
    Oidc { issuer: String },
    /// No issuer: the main listener is the unix socket, and every request is
    /// identified by its peer's kernel-reported uid.
    LocalPeer,
}

impl ServiceAuthMode {
    fn issuer(&self) -> Option<&str> {
        match self {
            Self::Oidc { issuer } => Some(issuer),
            Self::LocalPeer => None,
        }
    }

    /// The disposition every agent/registry/worker registration must carry
    /// in this mode.
    fn disposition(&self) -> Disposition {
        match self {
            Self::Oidc { .. } => Disposition::Intercepted,
            Self::LocalPeer => Disposition::LocalPeer,
        }
    }
}

/// Resolves the authentication mode once, at the single point the
/// requirement is established, rather than each registration site
/// re-deriving it (VPL-434-CORRECTNESS-3, VPL434-ARCH-2). `None` means the
/// service set has nothing to authenticate.
fn resolve_service_auth(
    services: StartupServices,
    issuer: Option<String>,
    main_port: Option<u16>,
) -> Result<Option<ServiceAuthMode>> {
    if anonymous_start_refused(services, issuer.as_deref(), main_port) {
        bail!(
            "agent, worker and archive/artifact services on a TCP or TLS listener require \
             --issuer for authentication; refusing to start unauthenticated — any peer that \
             can reach the listener could otherwise run arbitrary build entrypoints as this \
             process's uid, or have the agent read local files and push them to the registry \
             under this user's credentials. Without --issuer, omit --port and --tls to serve \
             them on the unix socket to peers running as this process's uid"
        );
    }

    if !services.requires_authentication() {
        return Ok(None);
    }

    Ok(Some(match issuer {
        Some(issuer) => ServiceAuthMode::Oidc { issuer },
        None => ServiceAuthMode::LocalPeer,
    }))
}

/// VPL-713 (C3): the registration invariant — every gRPC service this
/// process puts on the main router is either wrapped in the OIDC
/// interceptor or named in `EXEMPT_SERVICES` with a written reason.
///
/// The type is nested in its own module for the same compiler-enforcement
/// reason `resolved_registry` is: the `Router` field below has no visibility
/// modifier, so it is private to `service_registrar` and its descendants.
/// `run`, a sibling, cannot reach past the registrar to call
/// `Router::add_service` directly even though that method is public on
/// tonic's own type — a would-be bare registration in `run` fails to compile
/// rather than shipping unwrapped. That is the compiler-enforced form the
/// threat model's C3-d asks for; a grep gate over this file would have been
/// defeated by a rename or a new file.
///
/// The ledger `ledger()` returns is not a second list that has to be kept in
/// step with the router: `authenticated` and `exempt` each record and
/// register in the same call, so there is no state in which the router holds
/// a route the ledger does not describe.
mod service_registrar {
    use super::auth::{LocalPeerInterceptor, OidcInterceptor};
    use tonic::{
        codegen::{http, Service},
        server::NamedService,
        service::{interceptor::InterceptedService, Routes},
        transport::server::{Router, Server},
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum Disposition {
        /// Wrapped in the OIDC token interceptor.
        Intercepted,
        /// Wrapped in the unix-socket peer-uid interceptor.
        LocalPeer,
        Exempt,
    }

    /// The interceptor a service is registered behind. Each variant holds
    /// its own nominal interceptor type, so the disposition the ledger
    /// records follows from the interceptor that wraps the service and
    /// cannot be claimed for a different one.
    #[derive(Clone)]
    pub(super) enum ServiceAuth {
        Oidc(OidcInterceptor),
        LocalPeer(LocalPeerInterceptor),
    }

    /// Services allowed on the main router with no interceptor. Each entry
    /// carries the authority it does *not* hold, because that — not the
    /// service's convenience — is what makes an exemption defensible.
    ///
    /// Adding an entry here is a security change: it is the one way to make
    /// `every_registration_is_intercepted_or_exempt` pass for a service
    /// nobody authenticates. Review it as a change to the authentication
    /// boundary, not as a test fix.
    pub(super) const EXEMPT_SERVICES: &[(&str, &str)] = &[(
        "grpc.health.v1.Health",
        "reports SERVING/NOT_SERVING per registered service name and nothing \
         else: no request reaches an artifact store, a build entrypoint or a \
         registry namespace through it. gRPC health checking is specified for \
         unauthenticated probers (load balancers, kubelet), and the same \
         service is already served unauthenticated on its own plaintext port \
         when --health-check is set, so wrapping this copy would protect \
         nothing. Residual exposure: an unauthenticated peer learns which of \
         agent/registry/worker this process runs.",
    )];

    fn exemption_reason(name: &str) -> Option<&'static str> {
        EXEMPT_SERVICES
            .iter()
            .find(|(exempt, _)| *exempt == name)
            .map(|(_, reason)| *reason)
    }

    /// The single way a service reaches the main router.
    pub(super) struct ServiceRegistrar {
        router: Router,
        registered: Vec<(&'static str, Disposition)>,
    }

    impl ServiceRegistrar {
        /// Takes the configured `Server` builder rather than an already-built
        /// `Router`, so that no service — including the health service, which
        /// used to be the argument that turned the builder into a router —
        /// can reach the routes without passing `authenticated` or `exempt`.
        pub(super) fn new(mut server: Server) -> Self {
            Self {
                router: server.add_routes(Routes::default()),
                registered: Vec::new(),
            }
        }

        /// Registers `service` wrapped in `auth`'s interceptor. The wrapping
        /// happens here rather than at the call site, so a caller cannot hand
        /// this method a bare service and have it recorded as authenticated.
        pub(super) fn authenticated<S>(mut self, service: S, auth: ServiceAuth) -> Self
        where
            S: NamedService
                + Service<
                    http::Request<tonic::body::Body>,
                    Response = http::Response<tonic::body::Body>,
                    Error = std::convert::Infallible,
                > + Clone
                + Send
                + Sync
                + 'static,
            S::Future: Send + 'static,
        {
            let (disposition, router) = match auth {
                ServiceAuth::Oidc(interceptor) => (
                    Disposition::Intercepted,
                    self.router
                        .add_service(InterceptedService::new(service, interceptor)),
                ),
                ServiceAuth::LocalPeer(interceptor) => (
                    Disposition::LocalPeer,
                    self.router
                        .add_service(InterceptedService::new(service, interceptor)),
                ),
            };

            self.registered.push((S::NAME, disposition));
            self.router = router;
            self
        }

        /// Registers `service` with no interceptor. Refuses unless the
        /// service's gRPC name appears in `EXEMPT_SERVICES`, so the exemption
        /// list is the enforcement point rather than a description of one.
        pub(super) fn exempt<S>(mut self, service: S) -> anyhow::Result<Self>
        where
            S: NamedService
                + Service<
                    http::Request<tonic::body::Body>,
                    Response = http::Response<tonic::body::Body>,
                    Error = std::convert::Infallible,
                > + Clone
                + Send
                + Sync
                + 'static,
            S::Future: Send + 'static,
        {
            if exemption_reason(S::NAME).is_none() {
                anyhow::bail!(
                    "refusing to register {} without an authentication interceptor: it is not \
                     named in the exemption list in cli/src/command/start.rs",
                    S::NAME
                );
            }

            self.registered.push((S::NAME, Disposition::Exempt));
            self.router = self.router.add_service(service);
            Ok(self)
        }

        /// What this process actually registered, in registration order.
        pub(super) fn ledger(&self) -> &[(&'static str, Disposition)] {
            &self.registered
        }

        pub(super) fn into_router(self) -> Router {
            self.router
        }
    }
}

use service_registrar::{Disposition, ServiceAuth, ServiceRegistrar};

#[cfg(test)]
use service_registrar::EXEMPT_SERVICES;

/// What `run` registers on the main router for a given service set, as
/// service name and disposition, in registration order (VPL-713/C3-c).
///
/// This exists because the router itself cannot be asked: tonic 0.14's
/// `Routes` wraps a private `axum::Router` and exposes no accessor that
/// lists the service names it holds, and `NamedService::NAME` is a per-type
/// constant rather than something enumerable from a built router. So the
/// enumeration is a declaration. `run` compares it against
/// `ServiceRegistrar::ledger` before it binds a listener and bails when they
/// disagree, so a plan that drifts from what was registered stops the
/// process rather than becoming a comment that quietly goes stale.
///
/// Taking `StartupServices` and the resolved `ServiceAuthMode` rather than
/// `RunArgs` keeps this free of the network I/O `OidcValidator::new`
/// performs, so the enumeration test can cover every service subset in both
/// modes without an issuer to reach. The disposition comes from the mode,
/// independently of the interceptor `run` builds, so the plan-vs-ledger
/// comparison catches a registration wrapped for the wrong mode.
fn planned_registrations(
    services: StartupServices,
    mode: Option<&ServiceAuthMode>,
) -> Vec<(&'static str, Disposition)> {
    use tonic::server::NamedService;

    let mut planned = vec![(
        <HealthServer<HealthService> as NamedService>::NAME,
        Disposition::Exempt,
    )];

    let Some(mode) = mode else {
        return planned;
    };
    let disposition = mode.disposition();

    if services.has_agent {
        planned.push((
            <AgentServiceServer<AgentServer> as NamedService>::NAME,
            disposition,
        ));
    }

    if services.has_registry {
        planned.push((
            <ArchiveServiceServer<ArchiveServer> as NamedService>::NAME,
            disposition,
        ));
        planned.push((
            <ArtifactServiceServer<ArtifactServer> as NamedService>::NAME,
            disposition,
        ));
    }

    if services.has_worker {
        planned.push((
            <WorkerServiceServer<WorkerServer> as NamedService>::NAME,
            disposition,
        ));
    }

    planned
}

/// Whether every planned registration is either wrapped in an interceptor
/// (OIDC or local-peer) or named in `EXEMPT_SERVICES`.
///
/// Production enforcement of this rule is `ServiceRegistrar::exempt`, which
/// refuses an unlisted service at the registration statement itself. This is
/// the same rule restated over a plan, so the enumeration test can assert it
/// across every service set without standing up the backends and validator
/// those registrations would need.
#[cfg(test)]
fn registrations_are_intercepted_or_exempt(planned: &[(&'static str, Disposition)]) -> bool {
    planned.iter().all(|(name, disposition)| match disposition {
        Disposition::Intercepted | Disposition::LocalPeer => true,
        Disposition::Exempt => EXEMPT_SERVICES.iter().any(|(exempt, _)| exempt == name),
    })
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
/// an interceptor. `run` calls this once in OIDC mode and shares clones of
/// the returned interceptor across the agent, registry and worker
/// registrations, so every service uses one validator and one JWKS cache.
async fn new_validator_interceptor(
    issuer: &str,
    issuer_audience: Option<&str>,
    issuer_service_client_ids: &[String],
) -> Result<auth::OidcInterceptor> {
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

/// Adds the archive/artifact (registry) services to `registrar` when
/// `args.services` requests them, wrapped in the shared `auth` interceptor
/// (VPL-713/C3-f: one interceptor per `run`, not one per service block).
async fn add_registry_services(
    mut registrar: ServiceRegistrar,
    args: &RunArgs,
    auth: ServiceAuth,
    transport_label: &str,
) -> Result<ServiceRegistrar> {
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

    registrar = registrar.authenticated(
        ArchiveServiceServer::new(archive_server),
        // shared with the artifact service registered just below
        auth.clone(),
    );

    registrar = registrar.authenticated(ArtifactServiceServer::new(artifact_server), auth);

    info!("archive |> service: {}", transport_label);
    info!("artifact |> service: {}", transport_label);

    Ok(registrar)
}

/// Adds the worker service to `registrar` when `args.services` requests it,
/// wrapped in the shared `auth` interceptor (VPL-713/C3-f: one interceptor
/// per `run`, not one per service block). `issuer` is `None` in local-peer
/// mode, where the worker mints no service token for its registry dials.
fn add_worker_service(
    registrar: ServiceRegistrar,
    args: &RunArgs,
    issuer: Option<&str>,
    auth: ServiceAuth,
    registry_allowed: Vec<String>,
) -> ServiceRegistrar {
    // callee in start/worker.rs takes ownership; `args` is a shared reference reused below
    let worker_server = WorkerServer::new(
        issuer.map(str::to_string),
        args.issuer_audience.clone(),
        args.issuer_client_id.clone(),
        args.issuer_client_secret.clone(),
        registry_allowed,
        args.worker_jobs,
    );

    registrar.authenticated(WorkerServiceServer::new(worker_server), auth)
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

    // The dedicated health port serves exactly one service, and it goes
    // through the registrar for the same reason the main router's copy
    // does: `add_service` is not something `run` reaches directly, so a
    // future service added to this second listener has to declare itself
    // intercepted or exempt too.
    let health_router = ServiceRegistrar::new(Server::builder())
        .exempt(health_service_plaintext)?
        .into_router();

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

/// The main listener's TCP port: TLS implies TCP on 23151 by default, an
/// explicit `--port` also uses TCP, and `None` means the unix socket.
fn main_listener_port(port: Option<u16>, tls: bool) -> Option<u16> {
    match (port, tls) {
        (Some(port), _) => Some(port),
        (None, true) => Some(23151),
        (None, false) => None,
    }
}

/// Determines the effective listen port (`main_listener_port`), and rejects
/// a health-check port that collides with it.
fn resolve_effective_port(
    port: Option<u16>,
    tls: bool,
    health_check: bool,
    health_check_port: u16,
) -> Result<Option<u16>> {
    let effective_port = main_listener_port(port, tls);

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

#[expect(
    clippy::too_many_lines,
    reason = "the single top-level driver for every service registration \
              (add_registry_services/add_worker_service/prepare_health_check) threaded \
              through one ServiceRegistrar and one interceptor: splitting service \
              wiring across helper functions here would relocate the invariant that \
              every service is registered through the shared interceptor, not enforce \
              it more clearly"
)]
pub async fn run(args: RunArgs) -> Result<()> {
    log_trusted_service_clients(&args.issuer_service_client_ids);

    let startup_services = StartupServices::from_run_args(&args);
    let has_worker = startup_services.has_worker;
    let has_agent = startup_services.has_agent;
    let has_registry = startup_services.has_registry;

    let effective_port = resolve_effective_port(
        args.port,
        args.tls,
        args.health_check,
        args.health_check_port,
    )?;

    // VPL-434 (C1): refuse to start with an unauthenticated agent, worker or
    // registry (archive/artifact) service rather than defaulting into one.
    // The mode is resolved from `effective_port`, the same value the bind
    // below uses, so an issuer-less start can only serve the unix socket.
    let service_auth_mode =
        resolve_service_auth(startup_services, args.issuer.clone(), effective_port)?;

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

    // The agent's filesystem confinement boundary. Resolved before any
    // listener is bound so an unresolvable configured root refuses the start
    // rather than surfacing as a refused build later.
    let workspace_root_configured = args.workspace_root.is_some();
    let workspace_root = resolve_workspace_root(args.workspace_root.clone())?;

    // Same reason the registry allow-list is emitted above: what this process
    // will and will not read is a boot-time fact an operator should be able to
    // confirm. The cwd fallback in particular is only as confining as wherever
    // the process was started, so it is named as a fallback rather than
    // reported as configuration.
    if has_agent {
        if workspace_root_configured {
            info!("agent |> workspace root: {}", workspace_root.display());
        } else {
            warn!(
                "agent |> workspace root defaults to this process's working directory ({}); \
                 set --workspace-root/VORPAL_WORKSPACE_ROOT to confine it deliberately",
                workspace_root.display()
            );
        }
    }

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

    // Every service below reaches the router through `ServiceRegistrar`,
    // which either wraps it in the mode's interceptor or refuses unless its
    // gRPC name is in `EXEMPT_SERVICES` (VPL-713/C3). The health service is
    // the sole exemption today and takes the same `exempt` path any other
    // unwrapped registration would have to take.
    let builder = if args.tls {
        info!("TLS enabled for main listener");
        let tls_config = new_tls_config().await?;
        Server::builder().tls_config(tls_config)?
    } else {
        let transport = if effective_port.is_some() {
            "plaintext TCP"
        } else {
            "Unix domain socket"
        };
        info!("TLS disabled, using {} for main listener", transport);
        Server::builder()
    };

    let mut registrar = ServiceRegistrar::new(builder).exempt(health_service)?;

    let health_prepared = prepare_health_check(&args, &health_reporter).await?;

    let transport_label = match effective_port {
        Some(port) => format!("[::]:{port}"),
        None => get_socket_path().display().to_string(),
    };

    // `service_auth_mode` is `Some` here exactly when `has_agent`,
    // `has_registry` or `has_worker` is true (`resolve_service_auth`'s own
    // contract). Registering every one of them inside this arm, behind the
    // one interceptor the mode selects, is what leaves no branch in which a
    // service reaches the router without one; `ServiceRegistrar::authenticated`
    // is what makes that true of the registration itself rather than of this
    // `if`.
    if let Some(mode) = &service_auth_mode {
        // One interceptor for every wrapped service, constructed once
        // (VPL-713/C3-f). Three separate `new_validator_interceptor` calls
        // stood up three independent `OidcValidator`s — three startup
        // discovery and JWKS fetches, three caches refreshing on their own
        // schedules, and three argument lists a maintainer could edit apart
        // from each other so that one client ID classified as
        // `TrustedService` on the worker and `Human` on the registry.
        // Sharing one value makes that divergence unrepresentable.
        let service_auth = match mode {
            ServiceAuthMode::Oidc { issuer } => ServiceAuth::Oidc(
                new_validator_interceptor(
                    issuer,
                    args.issuer_audience.as_deref(),
                    &args.issuer_service_client_ids,
                )
                .await?,
            ),
            ServiceAuthMode::LocalPeer => {
                let interceptor = auth::LocalPeerInterceptor::new().map_err(|err| {
                    anyhow!(
                        "failed to resolve this process's uid for unix socket peer checks: {err}"
                    )
                })?;

                info!(
                    "no --issuer: agent, registry and worker services admit only peers on {} \
                     whose kernel-reported uid is {} (this process's uid)",
                    transport_label,
                    interceptor.service_uid()
                );

                ServiceAuth::LocalPeer(interceptor)
            }
        };

        if has_agent {
            registrar = registrar.authenticated(
                AgentServiceServer::new(AgentServer::new(
                    registry_allowed.clone(),
                    workspace_root.clone(),
                )),
                service_auth.clone(),
            );

            info!("agent |> service: {}", transport_label);
        }

        if has_registry {
            registrar =
                add_registry_services(registrar, &args, service_auth.clone(), &transport_label)
                    .await?;
        }

        if has_worker {
            worker::sweep_store_staging().await;

            registrar = add_worker_service(
                registrar,
                &args,
                mode.issuer(),
                service_auth.clone(),
                registry_allowed.clone(),
            );

            info!("worker |> service: {}", transport_label);
        }
    }

    // The ledger is what actually reached the router; the plan is what
    // `planned_registrations` declares for this service set and what the
    // enumeration test asserts over. Comparing them here is what keeps the
    // declaration honest: a registration added to `run` but not to the plan
    // (or removed from `run` and left in it) refuses to start rather than
    // shipping a mechanism whose own green test describes a router that no
    // longer exists.
    let planned = planned_registrations(startup_services, service_auth_mode.as_ref());

    if registrar.ledger() != planned.as_slice() {
        bail!(
            "registration plan does not describe what was registered: planned {:?}, registered {:?}",
            planned,
            registrar.ledger()
        );
    }

    let router = registrar.into_router();

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

    const TCP: Option<u16> = Some(23151);
    const UNIX_SOCKET: Option<u16> = None;

    // AC-1/AC-2 (VPL-434): a worker, registry (archive/artifact) or agent
    // service with no issuer on a TCP or TLS listener must be refused: no
    // peer identity exists there.
    #[test]
    fn anonymous_start_refused_for_worker_with_no_issuer_on_tcp() {
        assert!(anonymous_start_refused(
            services(true, false, false),
            None,
            TCP
        ));
    }

    #[test]
    fn anonymous_start_refused_for_registry_with_no_issuer_on_tcp() {
        assert!(anonymous_start_refused(
            services(false, true, false),
            None,
            TCP
        ));
    }

    #[test]
    fn anonymous_start_refused_for_agent_with_no_issuer_on_tcp() {
        assert!(anonymous_start_refused(
            services(false, false, true),
            None,
            TCP
        ));
    }

    // On the unix socket every caller carries a kernel-reported uid, so an
    // issuer-less start is not anonymous and is not refused.
    #[test]
    fn anonymous_start_not_refused_with_no_issuer_on_the_unix_socket() {
        assert!(!anonymous_start_refused(
            services(true, true, true),
            None,
            UNIX_SOCKET
        ));
    }

    #[test]
    fn anonymous_start_not_refused_when_an_issuer_is_configured() {
        for main_port in [TCP, UNIX_SOCKET] {
            assert!(!anonymous_start_refused(
                services(true, true, true),
                Some("https://issuer.example.com"),
                main_port
            ));
        }
    }

    // A process running none of the three has no surface for this predicate
    // to gate.
    #[test]
    fn anonymous_start_not_refused_for_a_process_running_none_of_them() {
        assert!(!anonymous_start_refused(
            services(false, false, false),
            None,
            TCP
        ));
    }

    // `--tls` with no `--port` binds TCP 23151, so it is refused like an
    // explicit port; the startup log's wrapper reads the same transport.
    #[test]
    fn issuerless_start_refused_for_tls_without_a_port() {
        let services = vec!["worker".to_string()];

        assert!(issuerless_start_refused(&services, None, true));
        assert!(issuerless_start_refused(&services, Some(4000), false));
        assert!(!issuerless_start_refused(&services, None, false));
    }
}

#[cfg(test)]
mod resolve_required_issuer_tests {
    use super::*;

    const ISSUER: &str = "https://issuer.example.com";

    fn services(has_worker: bool, has_registry: bool, has_agent: bool) -> StartupServices {
        StartupServices {
            has_worker,
            has_registry,
            has_agent,
        }
    }

    const TCP: Option<u16> = Some(23151);
    const UNIX_SOCKET: Option<u16> = None;

    fn oidc() -> ServiceAuthMode {
        ServiceAuthMode::Oidc {
            issuer: ISSUER.to_string(),
        }
    }

    #[test]
    fn agent_only_with_an_issuer_resolves_oidc_on_either_transport() {
        for main_port in [TCP, UNIX_SOCKET] {
            let resolved =
                resolve_service_auth(services(false, false, true), Some(ISSUER.into()), main_port);
            assert_eq!(resolved.ok(), Some(Some(oidc())));
        }
    }

    #[test]
    fn none_of_the_services_with_an_issuer_resolves_no_mode() {
        let resolved =
            resolve_service_auth(services(false, false, false), Some(ISSUER.into()), TCP);
        assert_eq!(resolved.ok(), Some(None));
    }

    // AC (derived): the full service set with no issuer, port or TLS starts
    // in local-peer mode.
    #[test]
    fn every_service_without_an_issuer_on_the_unix_socket_resolves_local_peer() {
        let resolved = resolve_service_auth(services(true, true, true), None, UNIX_SOCKET);
        assert_eq!(resolved.ok(), Some(Some(ServiceAuthMode::LocalPeer)));
    }

    #[test]
    fn agent_only_without_an_issuer_on_tcp_errors() {
        assert!(resolve_service_auth(services(false, false, true), None, TCP).is_err());
    }

    #[test]
    fn worker_only_without_an_issuer_on_tcp_errors() {
        assert!(resolve_service_auth(services(true, false, false), None, TCP).is_err());
    }

    #[test]
    fn registry_only_without_an_issuer_on_tcp_errors() {
        assert!(resolve_service_auth(services(false, true, false), None, TCP).is_err());
    }

    #[test]
    fn the_tcp_refusal_names_the_issuer_flag() {
        let resolved = resolve_service_auth(services(true, true, true), None, TCP);
        assert!(
            matches!(&resolved, Err(err) if err.to_string().contains("--issuer")),
            "refusal does not name --issuer: {resolved:?}"
        );
    }

    #[test]
    fn local_peer_mode_supplies_no_issuer_to_the_worker() {
        assert_eq!(ServiceAuthMode::LocalPeer.issuer(), None);
        assert_eq!(oidc().issuer(), Some(ISSUER));
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
#[expect(
    clippy::unwrap_used,
    reason = "test assertions read as intent, not defensive code: an unwrap failure is the test failing, which is the point"
)]
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
            workspace_root: None,
            services: vec![service.to_string()],
            tls: false,
            worker_jobs: 1,
        }
    }

    // A `run` that failed to refuse would bind and serve until a signal, so
    // the refusal tests bound their wait: a missing refusal fails the test
    // instead of hanging the suite.
    async fn run_until_refused(args: RunArgs) -> anyhow::Error {
        tokio::time::timeout(std::time::Duration::from_secs(10), run(args))
            .await
            .unwrap()
            .unwrap_err()
    }

    // An agent service with no issuer on a TCP port must return `Err` from
    // `run` without ever reaching a bind — the assertion is on `run`'s own
    // return value, not on the predicate it delegates to, so a mutation that
    // removes the call site (not just the predicate) fails this test.
    #[tokio::test]
    async fn run_refuses_an_agent_service_with_no_issuer_on_a_tcp_port() {
        let args = RunArgs {
            port: Some(23199),
            ..single_service_args("agent", None)
        };

        let err = run_until_refused(args).await;

        assert!(
            err.to_string().contains("require --issuer"),
            "unexpected error: {err}"
        );
    }

    // `--tls` alone binds TCP 23151, so it must be refused too — and before
    // `new_tls_config` reads any key, which is what would fail first if the
    // refusal read `--port` instead of the listener `run` binds.
    #[tokio::test]
    async fn run_refuses_an_agent_service_with_no_issuer_under_tls_without_a_port() {
        let args = RunArgs {
            tls: true,
            ..single_service_args("agent", None)
        };

        let err = run_until_refused(args).await;

        assert!(
            err.to_string().contains("require --issuer"),
            "unexpected error: {err}"
        );
    }

    // On the unix socket an issuer-less start proceeds past the refusal to
    // registration. The unknown registry backend stops it there, before any
    // listener binds, which proves the auth decision admitted the start.
    #[tokio::test]
    async fn run_starts_registering_services_with_no_issuer_on_the_unix_socket() {
        let args = RunArgs {
            registry_backend: "not-a-backend".to_string(),
            ..single_service_args("registry", None)
        };

        let err = run(args).await.unwrap_err();

        assert!(
            err.to_string().contains("unknown registry backend"),
            "run did not reach registration: {err}"
        );
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test assertions read as intent, not defensive code: an unwrap/expect/panic failure is the test failing, which is the point"
)]
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

    // The default the operator gets by omitting the flag, in the canonical
    // form the agent compares against — not the raw cwd string, which on a
    // symlinked path would refuse every request inside it.
    #[test]
    fn resolve_workspace_root_defaults_to_the_canonical_working_directory() {
        let expected = std::env::current_dir().unwrap().canonicalize().unwrap();

        assert_eq!(resolve_workspace_root(None).unwrap(), expected);
    }

    #[test]
    fn resolve_workspace_root_canonicalizes_a_configured_value() {
        let dir = tempfile::TempDir::new().unwrap();
        let expected = dir.path().canonicalize().unwrap();

        let configured = dir.path().join("..").join(
            dir.path()
                .file_name()
                .expect("the temp dir has a final component"),
        );

        assert_eq!(resolve_workspace_root(Some(configured)).unwrap(), expected);
    }

    // Fail closed at startup: an agent that cannot resolve its confinement
    // boundary must not come up serving requests without one.
    #[test]
    fn resolve_workspace_root_refuses_a_configured_root_that_does_not_exist() {
        let dir = tempfile::TempDir::new().unwrap();

        let err = resolve_workspace_root(Some(dir.path().join("missing"))).unwrap_err();

        assert!(
            err.to_string().contains("could not be resolved"),
            "unexpected error: {err}"
        );
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

// VPL-713 (C3, AC-2): the enumeration itself. `planned_registrations` names
// every service a given `RunArgs` service set puts on the main router, and
// `run` refuses to start when that plan disagrees with what
// `ServiceRegistrar` actually recorded, so these assertions are about the
// real router rather than a parallel description of one.
#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test assertions read as intent, not defensive code: an expect failure is the test failing, which is the point"
)]
mod registration_enumeration_tests {
    use super::*;

    fn services(has_worker: bool, has_registry: bool, has_agent: bool) -> StartupServices {
        StartupServices {
            has_worker,
            has_registry,
            has_agent,
        }
    }

    fn names(planned: &[(&'static str, Disposition)]) -> Vec<&'static str> {
        planned.iter().map(|(name, _)| *name).collect()
    }

    fn oidc() -> ServiceAuthMode {
        ServiceAuthMode::Oidc {
            issuer: "https://issuer.example.com".to_string(),
        }
    }

    // AC-2: every subset of the three configurable services, in both
    // authentication modes. No entry may be unwrapped unless the exemption
    // list names it.
    #[test]
    fn every_registration_is_intercepted_or_exempt() {
        for mode in [oidc(), ServiceAuthMode::LocalPeer] {
            for has_worker in [false, true] {
                for has_registry in [false, true] {
                    for has_agent in [false, true] {
                        let planned = planned_registrations(
                            services(has_worker, has_registry, has_agent),
                            Some(&mode),
                        );

                        assert!(
                            registrations_are_intercepted_or_exempt(&planned),
                            "unwrapped registration outside the exemption list for \
                             {mode:?} worker={has_worker} registry={has_registry} \
                             agent={has_agent}: {planned:?}"
                        );
                    }
                }
            }
        }
    }

    // The default `--services agent,registry,worker` install with an issuer:
    // four services behind the OIDC interceptor, health exempt. Pinning the
    // names, not just the count, is what catches a registration silently
    // swapped for a different service; pinning the disposition is what
    // catches one planned for the local-peer interceptor under an issuer.
    #[test]
    fn a_full_service_set_registers_four_intercepted_services_and_the_exempt_health_service() {
        let planned = planned_registrations(services(true, true, true), Some(&oidc()));

        assert_eq!(
            planned,
            vec![
                ("grpc.health.v1.Health", Disposition::Exempt),
                ("vorpal.agent.AgentService", Disposition::Intercepted),
                ("vorpal.archive.ArchiveService", Disposition::Intercepted),
                ("vorpal.artifact.ArtifactService", Disposition::Intercepted),
                ("vorpal.worker.WorkerService", Disposition::Intercepted),
            ]
        );
    }

    // The same install with no issuer on the unix socket: every service
    // behind the peer-uid interceptor, none behind OIDC, health exempt.
    #[test]
    fn a_full_service_set_in_local_peer_mode_registers_four_local_peer_services() {
        let planned = planned_registrations(
            services(true, true, true),
            Some(&ServiceAuthMode::LocalPeer),
        );

        assert_eq!(
            planned,
            vec![
                ("grpc.health.v1.Health", Disposition::Exempt),
                ("vorpal.agent.AgentService", Disposition::LocalPeer),
                ("vorpal.archive.ArchiveService", Disposition::LocalPeer),
                ("vorpal.artifact.ArtifactService", Disposition::LocalPeer),
                ("vorpal.worker.WorkerService", Disposition::LocalPeer),
            ]
        );
    }

    // The ledger side of the plan-vs-ledger comparison: the disposition the
    // registrar records follows from the interceptor it was handed.
    #[tokio::test]
    async fn the_registrar_records_a_local_peer_registration_as_local_peer() {
        let interceptor = auth::LocalPeerInterceptor::new()
            .expect("this process's uid resolves from its own socket pair");

        let registrar = ServiceRegistrar::new(Server::builder()).authenticated(
            AgentServiceServer::new(AgentServer::new(vec![], PathBuf::from("/"))),
            ServiceAuth::LocalPeer(interceptor),
        );

        assert_eq!(
            registrar.ledger(),
            [("vorpal.agent.AgentService", Disposition::LocalPeer)]
        );
    }

    // A process running none of the three still serves health, and health is
    // still the only thing it serves unwrapped.
    #[test]
    fn a_process_with_no_configured_services_registers_only_the_exempt_health_service() {
        let planned = planned_registrations(services(false, false, false), None);

        assert_eq!(names(&planned), vec!["grpc.health.v1.Health"]);
        assert!(registrations_are_intercepted_or_exempt(&planned));
    }

    // V-1 (the control-breaking check): the invariant predicate must actually
    // reject an unwrapped service the exemption list does not name. If this
    // passes, the enumeration above asserts nothing.
    #[test]
    fn an_unlisted_unwrapped_registration_is_rejected() {
        let smuggled = vec![
            ("grpc.health.v1.Health", Disposition::Exempt),
            ("vorpal.worker.WorkerService", Disposition::Exempt),
        ];

        assert!(!registrations_are_intercepted_or_exempt(&smuggled));
    }

    // AC-1/V-3: the exemption list's contents, asserted exactly. Widening it
    // is then a deliberate edit to this assertion — reviewable as the
    // security change it is — rather than a silently passing test.
    #[test]
    fn the_exemption_list_names_only_the_health_service() {
        let exempt: Vec<&str> = EXEMPT_SERVICES.iter().map(|(name, _)| *name).collect();

        assert_eq!(exempt, vec!["grpc.health.v1.Health"]);
    }

    // An exemption with no written reason is the laundering channel the list
    // exists to make visible, so an empty reason fails here.
    #[test]
    fn every_exemption_carries_a_written_reason() {
        for (name, reason) in EXEMPT_SERVICES {
            assert!(
                !reason.trim().is_empty(),
                "exemption {name} carries no reason"
            );
        }
    }

    // AC-1's refusal branch, at the production enforcement point rather than
    // over a plan: `exempt` must reject a service the list does not name, and
    // the error must say which service, so a maintainer reads what they are
    // being asked to justify.
    #[test]
    fn the_registrar_refuses_an_unwrapped_service_the_exemption_list_does_not_name() {
        let registrar = ServiceRegistrar::new(Server::builder());

        let err = registrar
            .exempt(AgentServiceServer::new(AgentServer::new(
                vec![],
                PathBuf::from("/"),
            )))
            .err()
            .expect("registering the agent with no interceptor must be refused");

        assert!(
            err.to_string().contains("vorpal.agent.AgentService"),
            "refusal does not name the service: {err}"
        );
    }

    // The same method must accept the one service the list does name,
    // otherwise the refusal above would pass for a registrar that refuses
    // everything.
    #[test]
    fn the_registrar_accepts_the_exempt_health_service() {
        let (_reporter, health_service) = tonic_health::server::health_reporter();

        let registrar = ServiceRegistrar::new(Server::builder())
            .exempt(health_service)
            .expect("the health service is named in the exemption list");

        assert_eq!(
            registrar.ledger(),
            [("grpc.health.v1.Health", Disposition::Exempt)]
        );
    }

    // AC-2 names `RunArgs`, so at least one case starts there: this pins the
    // `--services` string parsing in `StartupServices::from_run_args` as part
    // of the enumeration rather than assuming the flags.
    #[test]
    fn the_default_run_args_service_list_enumerates_every_service_as_intercepted_or_exempt() {
        let args = RunArgs {
            archive_cache_ttl: 3600,
            health_check: false,
            health_check_port: 0,
            issuer: Some("https://issuer.example.com".to_string()),
            issuer_audience: None,
            issuer_client_id: None,
            issuer_client_secret: None,
            issuer_service_client_ids: vec![],
            port: None,
            registry_backend: "local".to_string(),
            registry_backend_s3_bucket: None,
            registry_backend_s3_force_path_style: false,
            registry_allowed: None,
            workspace_root: None,
            services: vec![
                "agent".to_string(),
                "registry".to_string(),
                "worker".to_string(),
            ],
            tls: false,
            worker_jobs: 1,
        };

        let startup_services = StartupServices::from_run_args(&args);
        let mode = resolve_service_auth(startup_services, args.issuer.clone(), args.port)
            .expect("an issuer-configured start is never refused");
        let planned = planned_registrations(startup_services, mode.as_ref());

        assert_eq!(
            names(&planned),
            vec![
                "grpc.health.v1.Health",
                "vorpal.agent.AgentService",
                "vorpal.archive.ArchiveService",
                "vorpal.artifact.ArtifactService",
                "vorpal.worker.WorkerService",
            ]
        );
        assert!(registrations_are_intercepted_or_exempt(&planned));
    }

    // A `--services` value nothing recognises must register nothing beyond
    // health — never fall through to registering everything.
    #[test]
    fn an_unrecognised_service_name_registers_only_the_exempt_health_service() {
        let args = RunArgs {
            archive_cache_ttl: 3600,
            health_check: false,
            health_check_port: 0,
            issuer: None,
            issuer_audience: None,
            issuer_client_id: None,
            issuer_client_secret: None,
            issuer_service_client_ids: vec![],
            port: None,
            registry_backend: "local".to_string(),
            registry_backend_s3_bucket: None,
            registry_backend_s3_force_path_style: false,
            registry_allowed: None,
            workspace_root: None,
            services: vec!["workers".to_string()],
            tls: false,
            worker_jobs: 1,
        };

        let startup_services = StartupServices::from_run_args(&args);
        let mode = resolve_service_auth(startup_services, args.issuer.clone(), args.port)
            .expect("a start with nothing to authenticate is never refused");
        let planned = planned_registrations(startup_services, mode.as_ref());

        assert_eq!(names(&planned), vec!["grpc.health.v1.Health"]);
    }
}
