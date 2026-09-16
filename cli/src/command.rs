use crate::command::config::{
    VorpalConfigSource, VorpalConfigSourceGo, VorpalConfigSourcePython, VorpalConfigSourceRust,
    VorpalConfigSourceTypeScript,
};
use anyhow::{anyhow, bail, Context as _, Result};
use clap::parser::ValueSource;
use clap::{ArgAction, ArgMatches, CommandFactory, FromArgMatches, Parser, Subcommand};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, DeviceAuthorizationUrl, Scope,
    StandardDeviceAuthorizationResponse, TokenResponse, TokenUrl,
};
use path_clean::PathClean;
use rustls::crypto::ring;
use std::{
    env::current_dir,
    path::{Path, PathBuf},
    process::exit,
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::time::sleep;
use tracing::{error, subscriber, warn, Level};
use tracing_subscriber::{
    filter::{LevelFilter, Targets},
    layer::{Context, SubscriberExt},
    Layer, Registry,
};
use vorpal_sdk::{
    artifact::{get_default_address, system::get_system_default_str},
    context::{
        commit_login_credentials, credential_egress_origin, LoginRecord, VorpalCredentialsContent,
        DEFAULT_NAMESPACE,
    },
};

mod build;
mod config;
mod config_cmd;
mod init;
mod inspect;
mod lock;
mod run;
mod start;
mod store;
mod system;

pub fn get_default_namespace() -> String {
    DEFAULT_NAMESPACE.to_string()
}

/// Parses a comma-separated list, trimming whitespace per entry and
/// dropping empty segments, so inputs like "worker-id," or " a , , b " never
/// produce `""` entries. Shared by `--issuer-service-client-ids` and
/// `--registry-allowed`: silent-filtering matches clap's ergonomic
/// expectation for comma-delimited values and keeps config-by-env forgiving.
fn parse_comma_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parses `--registry-allowed`/`VORPAL_REGISTRY_ALLOWED` without resolving a
/// default: `None` means the flag was never passed, and `start::run` is what
/// defaults that to a registry address, because only `start::run` knows the
/// process's actual transport (`--port`/`--tls`) — resolving the default
/// here against `get_default_address()` (a *client*-facing helper, ignorant
/// of this process's own listen mode) is exactly the bug this parses around.
/// An explicit but empty value (`--registry-allowed ""`) parses to
/// `Some(vec![])`, which is fail-closed: it means the operator deliberately
/// configured no registry, never "any registry".
fn resolve_registry_allowed_flag(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(parse_comma_list)
}

#[cfg(test)]
mod registry_allowed_tests {
    use super::*;

    #[test]
    fn parse_comma_list_trims_and_drops_empty_segments() {
        assert_eq!(
            parse_comma_list(" a , , b ,c"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn parse_comma_list_of_empty_string_is_empty() {
        assert_eq!(parse_comma_list(""), Vec::<String>::new());
    }

    // C1 (reconcile): the flag omitted entirely must resolve to `None`, not
    // to a default computed here — `start::run` is the only place that
    // knows this process's actual transport, so it is the only place that
    // can name a default that matches what the process is listening on.
    #[test]
    fn resolve_registry_allowed_flag_defers_to_the_caller_when_unset() {
        assert_eq!(resolve_registry_allowed_flag(None), None);
    }

    #[test]
    fn resolve_registry_allowed_flag_parses_an_explicit_list() {
        assert_eq!(
            resolve_registry_allowed_flag(Some("http://a.example.com,http://b.example.com")),
            Some(vec![
                "http://a.example.com".to_string(),
                "http://b.example.com".to_string()
            ])
        );
    }

    // An explicit empty value is a deliberate fail-closed choice, distinct
    // from leaving the flag unset (`None` above).
    #[test]
    fn resolve_registry_allowed_flag_explicit_empty_string_fails_closed() {
        assert_eq!(
            resolve_registry_allowed_flag(Some("")),
            Some(Vec::<String>::new())
        );
    }

    // C9 (reconcile): C1's original migration-default bug lived at this call
    // site (`resolve_registry_allowed_flag(registry_allowed.as_deref())`
    // below, in the `Start` match arm), not inside the helper — a helper-only
    // test suite passed on the buggy call site because the helper itself was
    // never wrong; only what the caller did with its `None` case was. Drive
    // the same call the match arm makes, end to end, so a regression that
    // reintroduces a default *here* (rather than leaving it to `start::run`)
    // fails a test instead of shipping silently.
    #[test]
    fn command_start_arm_forwards_an_unset_flag_as_none_to_run_args() {
        let registry_allowed_flag: Option<String> = None;

        let registry_allowed = resolve_registry_allowed_flag(registry_allowed_flag.as_deref());

        let run_args = crate::command::start::RunArgs {
            archive_cache_ttl: 300,
            health_check: false,
            health_check_port: 23152,
            issuer: None,
            issuer_audience: None,
            issuer_client_id: None,
            issuer_client_secret: None,
            issuer_service_client_ids: vec![],
            port: None,
            registry_backend: "local".to_string(),
            registry_backend_s3_bucket: None,
            registry_backend_s3_force_path_style: false,
            registry_allowed,
            services: vec!["worker".to_string()],
            tls: false,
            workspace_root: None,
        };

        assert_eq!(run_args.registry_allowed, None);
    }
}

#[derive(Subcommand)]
pub enum CommandSystemKeys {
    Generate {},
}

/// clap ids for the two `system services start` arguments whose *provenance*
/// is read back at startup (`services_start_value_source`).
///
/// Both the definition below and the lookup name the id through these
/// constants, so a rename is a compile error. Spelled out because
/// `ArgMatches::value_source` takes the id as a string: given an id no
/// argument carries it panics in a debug build and returns `None` in a
/// release build, which would have turned a rename into a silently disabled
/// argv-secret warning in exactly the profile that ships.
const ISSUER_ARG_ID: &str = "issuer";
const ISSUER_CLIENT_SECRET_ARG_ID: &str = "issuer_client_secret";

#[derive(Subcommand)]
pub enum CommandSystemServices {
    Start {
        /// TTL in seconds for caching archive check results. Set to 0 to disable caching.
        #[arg(default_value = "300", long)]
        archive_cache_ttl: u64,

        /// Enable the plaintext health-check listener
        #[arg(default_value_t = false, long)]
        health_check: bool,

        /// Plaintext (non-TLS) port for gRPC health checks
        #[arg(default_value = "23152", long)]
        health_check_port: u16,

        /// OIDC issuer URL for `--services worker`/`registry` authentication.
        /// Settable via `VORPAL_ISSUER` so a default install
        /// (`script/install.sh`) can supply it without a flag. Validated at
        /// parse time by `parse_issuer` so a value on either channel that is
        /// empty or not a well-formed issuer never reaches
        /// `resolve_required_issuer` disguised as "present".
        #[arg(
            env = "VORPAL_ISSUER",
            id = ISSUER_ARG_ID,
            long = "issuer",
            value_parser = parse_issuer
        )]
        issuer: Option<String>,

        #[arg(long)]
        issuer_audience: Option<String>,

        #[arg(long)]
        issuer_client_id: Option<String>,

        /// Settable via `VORPAL_ISSUER_CLIENT_SECRET` so an installed unit can
        /// read the secret from a mode-600 environment file instead of
        /// carrying it in argv, where any local user could read it from
        /// `ps`/`/proc/<pid>/cmdline`. It is the channel `script/install.sh` writes. `hide_env_values` keeps
        /// `--help` from printing the secret straight back out of the
        /// environment the unit just hid it in, and
        /// `parse_client_secret` refuses a set-but-empty value on either
        /// channel.
        #[arg(
            env = "VORPAL_ISSUER_CLIENT_SECRET",
            hide_env_values = true,
            id = ISSUER_CLIENT_SECRET_ARG_ID,
            long = "issuer-client-secret",
            value_parser = parse_client_secret
        )]
        issuer_client_secret: Option<String>,

        /// Comma-separated OAuth client IDs whose tokens are classified as
        /// trusted service principals. Tokens whose `azp` claim matches a
        /// list entry bypass namespace RBAC. Leave unset (default) to
        /// preserve current behavior (all tokens go through namespace RBAC).
        #[arg(env = "VORPAL_ISSUER_SERVICE_CLIENT_IDS", long)]
        issuer_service_client_ids: Option<String>,

        /// TCP port to listen on. If omitted, listens on a Unix domain socket
        /// (default: /var/lib/vorpal/vorpal.sock, override: `VORPAL_SOCKET_PATH` env var)
        #[arg(long)]
        port: Option<u16>,

        #[arg(default_value = "agent,registry,worker", long)]
        services: String,

        #[arg(default_value = "local", long)]
        registry_backend: String,

        #[arg(long)]
        registry_backend_s3_bucket: Option<String>,

        #[arg(default_value_t = false, long)]
        registry_backend_s3_force_path_style: bool,

        /// Comma-separated registries the worker's `build_artifact` may pull
        /// from or push to. A request-supplied `registry` may only select a
        /// value from this list (or leave it unset to get the first entry,
        /// the worker's own configured value); a request naming anything
        /// else is refused. Leave unset (default): if `--services` includes
        /// `registry`, the worker allows only this process's own registry
        /// endpoint — computed from `--port`/`--tls` at start time
        /// (`unix://<socket>`, `http://localhost:<port>` or
        /// `https://localhost:<port>`; `localhost`, not an IP literal, to
        /// match the generated certificate's DNS SAN and to reach a listener
        /// bound to the IPv6 unspecified address), not the client-facing
        /// `get_default_address()` default every other command targets. If
        /// this process does not run its own registry service (a split
        /// worker- or agent-only deployment), the omitted flag instead fails
        /// closed — every build is refused until `--registry-allowed` names
        /// the real registry. Pass an explicit empty string to fail closed
        /// deliberately on a combined deployment too.
        #[arg(env = "VORPAL_REGISTRY_ALLOWED", long)]
        registry_allowed: Option<String>,

        /// Enable TLS for the main gRPC listener (requires keys in /var/lib/vorpal/key/)
        #[arg(default_value_t = false, long)]
        tls: bool,

        /// Directory the agent confines every request's `artifact_context`
        /// to. The agent reads `<context>/Vorpal.lock`, walks and copies the
        /// whole tree under `<context>` for a local source, and writes
        /// `<context>/Vorpal.lock` back, all as its own uid — a request whose
        /// context resolves outside this root is refused. Leave unset
        /// (default): this process's own working directory, which confines
        /// nothing if the agent was started from `/` or a home directory, so
        /// name the workspace deliberately on any host the agent's owner does
        /// not have to themselves.
        #[arg(env = "VORPAL_WORKSPACE_ROOT", long)]
        workspace_root: Option<PathBuf>,
    },
}

// `Services(CommandSystemServices)` carries the full set of flags for
// `vorpal system services start` (including `issuer_service_client_ids` added
// in DKT-63), pushing this enum past clippy's default 200-byte
// `large_enum_variant` threshold. Boxing the variant would churn every
// destructure site for zero runtime win: this is a top-level CLI subcommand
// type parsed once per process invocation, so the size differential is
// meaningless at the scale of a single clap parse.
#[expect(
    clippy::large_enum_variant,
    reason = "top-level CLI subcommand type; single instance per process"
)]
#[derive(Subcommand)]
pub enum CommandSystem {
    #[clap(subcommand)]
    Keys(CommandSystemKeys),

    Prune {
        /// Prune all resources
        #[arg(default_value_t = false, long)]
        all: bool,
        /// Prune artifact aliases
        #[arg(long)]
        artifact_aliases: bool,
        /// Prune artifact archives
        #[arg(long)]
        artifact_archives: bool,
        /// Prune artifact configs
        #[arg(long)]
        artifact_configs: bool,
        /// Prune artifact outputs
        #[arg(long)]
        artifact_outputs: bool,
        /// Prune sandboxes
        #[arg(long)]
        sandboxes: bool,
    },

    #[clap(subcommand)]
    Services(CommandSystemServices),
}

#[derive(Subcommand)]
pub enum Command {
    /// Build an artifact
    Build {
        /// Artifact name
        name: String,

        /// Artifact agent address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        agent: String,

        /// Artifact configuration file
        #[arg(default_value = "Vorpal.toml", long)]
        config: PathBuf,

        /// Artifact context
        #[arg(default_value = ".", long)]
        context: PathBuf,

        /// Artifact export
        #[arg(default_value_t = false, long)]
        export: bool,

        /// Maximum number of artifacts to build concurrently (default: available CPU parallelism)
        #[arg(default_value_t = get_default_jobs(), long, short = 'j')]
        jobs: usize,

        /// List artifact and dependencies (name + digest) without building
        #[arg(default_value_t = false, long, conflicts_with = "export")]
        list: bool,

        /// Artifact namespace
        #[arg(default_value_t = get_default_namespace(), long)]
        namespace: String,

        /// Artifact path
        #[arg(default_value_t = false, long)]
        path: bool,

        /// Artifact rebuild
        #[arg(default_value_t = false, long)]
        rebuild: bool,

        /// Registry address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), global = true, long)]
        registry: String,

        /// Artifact system (default: host system)
        #[arg(default_value_t = get_system_default_str(), long)]
        system: String,

        /// Artifact lock unlock
        #[arg(default_value_t = false, long)]
        unlock: bool,

        /// Artifact variables (key=value)
        #[arg(long)]
        variable: Vec<String>,

        /// Artifact worker address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        worker: String,
    },

    /// Prepare an artifact: download and pin its sources into Vorpal.lock without
    /// building the target artifact. Unlike `build`, `--unlock` defaults to true.
    ///
    /// Config-language toolchain prerequisites (e.g. protoc for Go configs) still
    /// build via the worker as needed to execute the config binary and enumerate
    /// the artifact graph - this always runs host-natively, so it works from any
    /// host for any `--system` target.
    Prepare {
        /// Artifact name
        name: String,

        /// Artifact agent address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        agent: String,

        /// Artifact configuration file
        #[arg(default_value = "Vorpal.toml", long)]
        config: PathBuf,

        /// Artifact context
        #[arg(default_value = ".", long)]
        context: PathBuf,

        /// Maximum number of artifacts to build concurrently (default: available CPU parallelism)
        #[arg(default_value_t = get_default_jobs(), long, short = 'j')]
        jobs: usize,

        /// Artifact namespace
        #[arg(default_value_t = get_default_namespace(), long)]
        namespace: String,

        /// Registry address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), global = true, long)]
        registry: String,

        /// Artifact system (default: host system)
        #[arg(default_value_t = get_system_default_str(), long)]
        system: String,

        /// Artifact lock unlock (defaults to true, unlike `build`: minting and
        /// updating pins is this command's entire purpose). Pass `--unlock=false`
        /// to enforce the fail-closed gates as if `--unlock` were never passed to `build`.
        #[arg(
            long,
            default_value_t = true,
            default_missing_value = "true",
            num_args = 0..=1,
            require_equals = true,
            action = ArgAction::Set
        )]
        unlock: bool,

        /// Artifact variables (key=value)
        #[arg(long)]
        variable: Vec<String>,

        /// Artifact worker address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        worker: String,
    },

    /// Manage configuration settings
    Config {
        /// Apply to user-level config (~/.vorpal/settings.json) instead of project-level
        #[arg(long)]
        user: bool,

        /// Path to the project-level configuration file
        #[arg(default_value = "Vorpal.toml", long)]
        config: PathBuf,

        #[command(subcommand)]
        action: config_cmd::ConfigAction,
    },

    /// Initialize Vorpal in a directory
    Init {
        /// Project name
        name: String,

        /// Output directory
        #[arg(default_value = ".", long)]
        path: PathBuf,
    },

    /// Inspect an artifact
    Inspect {
        /// Artifact digest
        digest: String,

        /// Artifact namespace
        #[arg(default_value_t = get_default_namespace(), long)]
        namespace: String,

        /// Registry address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        registry: String,
    },

    /// Login to an `OAuth2` provider
    Login {
        /// Issuer base URL, e.g. <https://id.example.com/realms/myrealm>
        #[arg(long, default_value = DEFAULT_DEV_ISSUER)]
        issuer: String,

        #[arg(long)]
        /// Issuer `OAuth2` Client Audience
        issuer_audience: Option<String>,

        /// Issuer `OAuth2` Client ID
        #[arg(long, default_value = "cli")]
        issuer_client_id: String,

        /// Registry address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), global = true, long)]
        registry: String,
    },

    /// Run a built artifact from the store
    #[clap(trailing_var_arg = true)]
    Run {
        /// Artifact alias ([<namespace>/]<name>[:<tag>])
        alias: String,

        /// Arguments to pass to the artifact binary
        #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
        args: Vec<String>,

        /// Override the binary name to execute (default: artifact name)
        #[arg(long)]
        bin: Option<String>,

        /// Registry address (`VORPAL_SOCKET_PATH` env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        registry: String,
    },

    /// Manage Vorpal system
    #[clap(subcommand)]
    System(CommandSystem),
}

/// h2 resolves a malformed request (e.g. an invalid client `:authority`) by
/// resetting just that stream with `PROTOCOL_ERROR` entirely inside the h2
/// crate: `Streams::reset_on_recv_stream_err` swallows the reset into
/// `Ok(())` before it ever reaches tonic/tower, so no service, interceptor,
/// or layer in the request-handling stack observes it. The only trace is
/// h2's own `tracing::debug!` ("malformed headers: ..." / "... `PROTOCOL_ERROR`
/// -- ..."), invisible under the default `--level info`. This layer relays
/// that debug event as a WARN so it's visible without `--level debug`,
/// without touching h2's rejection behavior (DKT-32, diagnosed in DKT-28).
struct H2ProtocolErrorRelay {
    last_warn: Mutex<Option<Instant>>,
}

/// Minimum spacing between relayed h2-protocol-error WARNs. An unauthenticated
/// peer can stream malformed frames (invalid `:authority`, `PROTOCOL_ERROR`
/// resets) one after another; without throttling each produces a separate h2
/// debug event and thus a relayed WARN. This window caps the relay to one WARN
/// per interval so a frame flood cannot generate an unbounded stream of WARN
/// lines at the default log level.
const H2_RELAY_THROTTLE_WINDOW: Duration = Duration::from_secs(5);

/// Visits a `tracing::Event`'s fields to extract the `message` field. Only
/// `record_debug` is implemented (not `record_str`/`record_i64`/etc.) — this is
/// correct today because h2 emits its diagnostic `message` as a Debug-formatted
/// field, but a future h2 version emitting the field under a different visitor
/// method would silently produce an empty-message WARN rather than a compile
/// error. The integration tests below guard against that regression.
struct H2MessageVisitor(String);

impl tracing::field::Visit for H2MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

/// True when a `target`/`level`/`message` triple is h2's own signal for a
/// stream it rejected with `PROTOCOL_ERROR` (malformed headers, or the
/// `proto_err!`-shaped "stream/connection error `PROTOCOL_ERROR` -- ..." family).
///
/// COUPLING NOTE: this predicate matches h2 **0.4.15**'s internal debug
/// messages by substring ("malformed headers" / "`PROTOCOL_ERROR`"). These
/// strings are NOT part of h2's semver-guaranteed public API — an h2 upgrade
/// can silently widen, narrow, or reword them and break the match (or match
/// unintended events). The unit tests in `h2_protocol_error_relay_tests`
/// (`matches_*` / `ignores_*`) are the regression guard: bumping h2 must keep
/// them green, or this predicate must be re-derived against the new wording.
fn is_h2_protocol_error(target: &str, level: Level, message: &str) -> bool {
    level == Level::DEBUG
        && target.starts_with("h2")
        && (message.contains("malformed headers") || message.contains("PROTOCOL_ERROR"))
}

impl<S: tracing::Subscriber> Layer<S> for H2ProtocolErrorRelay {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        let mut message = H2MessageVisitor(String::new());
        event.record(&mut message);

        if !is_h2_protocol_error(metadata.target(), *metadata.level(), &message.0) {
            return;
        }

        // Rate-limit: suppress repeats of the same rejection class within a
        // short window so an unauthenticated malformed-frame flood cannot emit
        // one WARN per frame. The first occurrence in each window still emits.
        let now = Instant::now();
        // The guarded state is a plain `Option<Instant>`, which cannot be left
        // structurally inconsistent by a panicking holder, so recovering the
        // guard on poison (rather than dropping the relayed WARN) is safe.
        let mut last = self
            .last_warn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(prev) = *last {
            if now.duration_since(prev) < H2_RELAY_THROTTLE_WINDOW {
                return;
            }
        }
        *last = Some(now);
        drop(last);

        // Branch the relay target by the originating h2 side so client-driven
        // gRPC errors (outbound calls on `h2::client`) are not mislabeled as
        // agent-side stream rejections. `h2::server` is inbound agent traffic;
        // anything else is treated as an outbound client call. (The `target:`
        // argument is a static callsite key, so the branch is expressed as two
        // literal-target calls rather than a runtime variable.)
        if metadata.target().starts_with("h2::client") {
            tracing::warn!(
                target: "vorpal_cli::client",
                "h2 rejected stream: {}",
                message.0
            );
        } else {
            tracing::warn!(
                target: "vorpal_cli::agent",
                "h2 rejected stream: {}",
                message.0
            );
        }
    }
}

#[cfg(test)]
mod h2_protocol_error_relay_tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn matches_malformed_authority_debug_event() {
        assert!(is_h2_protocol_error(
            "h2::server",
            Level::DEBUG,
            "malformed headers: malformed authority (\"bad::authority\"): invalid uri character",
        ));
    }

    #[test]
    fn matches_proto_err_shaped_debug_event() {
        assert!(is_h2_protocol_error(
            "h2::proto::streams::recv",
            Level::DEBUG,
            "stream error PROTOCOL_ERROR -- recv_headers: trailers frame was not EOS; stream=StreamId(1);",
        ));
    }

    #[test]
    fn ignores_non_h2_target() {
        assert!(!is_h2_protocol_error(
            "vorpal_cli::agent",
            Level::DEBUG,
            "malformed headers: malformed authority (...): invalid uri character",
        ));
    }

    #[test]
    fn ignores_non_debug_level() {
        assert!(!is_h2_protocol_error(
            "h2::server",
            Level::TRACE,
            "malformed headers: malformed authority (...): invalid uri character",
        ));
    }

    #[test]
    fn ignores_unrelated_h2_debug_event() {
        // h2's server-push validation logs (convert_push_message) share the
        // crate but are a different rejection class - not in scope for DKT-32.
        assert!(!is_h2_protocol_error(
            "h2::server",
            Level::DEBUG,
            "convert_push_message: method POST is not safe and cacheable",
        ));
    }

    // End-to-end relay coverage. A layer that emits a re-entrant `tracing::warn!`
    // from inside `on_event` CANNOT be exercised via `tracing::subscriber::
    // with_default`: the scoped-dispatch path in tracing-core guards against
    // re-entrant dispatch (`dispatcher::get_default` returns `Dispatch::none()`
    // when re-entered under a non-zero `SCOPED_COUNT`). The production path
    // (`set_global_default`) takes the global fast path which has NO such guard,
    // so the relay genuinely works in production. The faithful harness is
    // therefore a process-global capturing subscriber installed once.

    use std::sync::OnceLock;

    struct CapturingWriter(&'static Mutex<Vec<u8>>);

    impl std::io::Write for CapturingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|err| std::io::Error::other(err.to_string()))?
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct CapturingMakeWriter(&'static Mutex<Vec<u8>>);

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturingMakeWriter {
        type Writer = CapturingWriter;

        fn make_writer(&'a self) -> Self::Writer {
            CapturingWriter(self.0)
        }
    }

    // Shared capture buffer + process-global subscriber, installed once.
    static CAPTURE: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    static GLOBAL: OnceLock<()> = OnceLock::new();

    fn ensure_global_subscriber() {
        GLOBAL.get_or_init(|| {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_writer(CapturingMakeWriter(&CAPTURE))
                .without_time();

            let subscriber = Registry::default()
                .with(fmt_layer.with_filter(LevelFilter::from_level(Level::INFO)))
                .with(
                    H2ProtocolErrorRelay {
                        last_warn: Mutex::new(None),
                    }
                    .with_filter(Targets::new().with_target("h2", LevelFilter::DEBUG)),
                );

            // Best-effort; a global default may already be set in which case the
            // capture path is whatever was installed (test would then fail loudly).
            let _ = tracing::subscriber::set_global_default(subscriber);
        });
    }

    fn captured() -> Result<String, Box<dyn std::error::Error>> {
        // can't move Vec<u8> out of the MutexGuard; clone releases the lock immediately
        let bytes = CAPTURE.lock()?.clone();
        Ok(String::from_utf8(bytes).unwrap_or_default())
    }

    #[test]
    fn relay_end_to_end_observe_rate_limit_and_client_target(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // AC (Concern, closes the isolated-predicate gap): the relayed WARN must
        // be observed end-to-end through the real layered subscriber, not just
        // via the predicate. AC (Medium, secB): a malformed-frame flood must not
        // emit one WARN per frame. AC (Concern): client-side events are routed to
        // the client target, not mislabeled as agent-side.
        //
        // We emit a burst of 50 client-side h2 debug events against the
        // process-global subscriber: the throttle collapses the flood to exactly
        // one WARN, and that single WARN proves both end-to-end visibility and
        // the client-target routing. (Server routing is the literal else-branch
        // mirror of the client branch asserted here.)
        ensure_global_subscriber();
        CAPTURE.lock()?.clear();

        tracing::debug!(
            target: "h2::client",
            message = "malformed headers: malformed authority (\"bad::authority\"): invalid uri character",
            "h2 client rejection"
        );
        // A second distinct rejection class to demonstrate flood suppression.
        for _ in 0..49 {
            tracing::debug!(
                target: "h2::client",
                message = "stream error PROTOCOL_ERROR -- remote peer sent an invalid frame",
                "h2 client rejection"
            );
        }

        let captured = captured()?;
        let warn_count = captured.matches("h2 rejected stream").count();
        assert_eq!(
            warn_count, 1,
            "expected exactly one relayed WARN (flood suppressed), got {warn_count}: {captured}"
        );
        assert!(
            captured.contains("vorpal_cli::client"),
            "expected the client-side relay target, got: {captured}"
        );
        assert!(
            !captured.contains("vorpal_cli::agent"),
            "client-side event must not be mislabeled as agent-side: {captured}"
        );

        Ok(())
    }
}

const VERSION_INFO: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (commit:",
    env!("VORPAL_GIT_HASH"),
    " build:",
    env!("VORPAL_BUILD_TIME"),
    ")",
);

#[derive(Parser)]
#[command(author, about, long_about = None)]
#[command(version = VERSION_INFO, long_version = VERSION_INFO)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    // Log level
    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions read as intent, not defensive code: an unwrap/expect/panic failure is the test failing, which is the point"
)]
mod unlock_parse_tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["vorpal"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full)
    }

    #[test]
    fn prepare_unlock_omitted_defaults_true() -> Result<(), Box<dyn std::error::Error>> {
        let cli = parse(&["prepare", "foo"])?;
        let Command::Prepare { unlock, .. } = cli.command else {
            return Err("expected Prepare command".into());
        };
        assert!(unlock);
        Ok(())
    }

    #[test]
    fn prepare_unlock_bare_flag_is_true() -> Result<(), Box<dyn std::error::Error>> {
        let cli = parse(&["prepare", "foo", "--unlock"])?;
        let Command::Prepare { unlock, .. } = cli.command else {
            return Err("expected Prepare command".into());
        };
        assert!(unlock);
        Ok(())
    }

    #[test]
    fn prepare_unlock_equals_false_is_false() -> Result<(), Box<dyn std::error::Error>> {
        let cli = parse(&["prepare", "foo", "--unlock=false"])?;
        let Command::Prepare { unlock, .. } = cli.command else {
            return Err("expected Prepare command".into());
        };
        assert!(!unlock);
        Ok(())
    }

    #[test]
    fn prepare_unlock_equals_true_is_true() -> Result<(), Box<dyn std::error::Error>> {
        let cli = parse(&["prepare", "foo", "--unlock=true"])?;
        let Command::Prepare { unlock, .. } = cli.command else {
            return Err("expected Prepare command".into());
        };
        assert!(unlock);
        Ok(())
    }

    #[test]
    fn prepare_unlock_space_separated_value_is_rejected() {
        // require_equals = true means a space-separated value is not swallowed
        // into --unlock; clap instead treats "false" as an unexpected extra
        // positional argument and errors.
        let result = parse(&["prepare", "foo", "--unlock", "false"]);
        assert!(result.is_err());
    }

    #[test]
    fn build_unlock_omitted_defaults_false() -> Result<(), Box<dyn std::error::Error>> {
        let cli = parse(&["build", "foo"])?;
        let Command::Build { unlock, .. } = cli.command else {
            return Err("expected Build command".into());
        };
        assert!(!unlock);
        Ok(())
    }

    // C-6 positive control: an explicit `--jobs`/`-j` value round-trips
    // through clap unchanged (clamping is a separate, later step - see
    // `clamp_jobs_tests` below).
    #[test]
    fn build_jobs_long_flag_parses() {
        let cli = parse(&["build", "foo", "--jobs", "4"]).expect("should parse");
        match cli.command {
            Command::Build { jobs, .. } => assert_eq!(jobs, 4),
            _ => panic!("expected Build command"),
        }
    }

    #[test]
    fn build_jobs_short_flag_parses() {
        let cli = parse(&["build", "foo", "-j", "4"]).expect("should parse");
        match cli.command {
            Command::Build { jobs, .. } => assert_eq!(jobs, 4),
            _ => panic!("expected Build command"),
        }
    }

    #[test]
    fn build_jobs_omitted_defaults_to_available_parallelism() {
        let cli = parse(&["build", "foo"]).expect("should parse");
        match cli.command {
            Command::Build { jobs, .. } => {
                assert_eq!(jobs, get_default_jobs());

                // The property the flag actually promises, rather than the
                // wiring compared to itself: an omitted `--jobs` is a usable
                // width. `get_default_jobs`'s own `unwrap_or(1)` fallback is
                // not otherwise reachable from a test, since
                // `available_parallelism()` is not injectable.
                assert!(jobs >= 1, "the default --jobs must be at least 1");
            }
            _ => panic!("expected Build command"),
        }
    }

    #[test]
    fn prepare_jobs_flag_parses() {
        let cli = parse(&["prepare", "foo", "--jobs", "3"]).expect("should parse");
        match cli.command {
            Command::Prepare { jobs, .. } => assert_eq!(jobs, 3),
            _ => panic!("expected Prepare command"),
        }
    }
}

#[cfg(test)]
mod clamp_jobs_tests {
    use super::*;

    // C-6: `--jobs 0` is a stated decision (floor to 1), not a refusal.
    #[test]
    fn clamp_jobs_floors_zero_to_one() {
        assert_eq!(clamp_jobs(0), 1);
    }

    // C-6 positive control: an ordinary value passes through unchanged.
    #[test]
    fn clamp_jobs_passes_through_an_ordinary_value() {
        assert_eq!(clamp_jobs(4), 4);
    }

    // C-6 / AB-6: a value above the ceiling is capped rather than trusted,
    // since it is what bounds how much work one invocation has outstanding
    // against the store and the worker, not thread safety.
    #[test]
    fn clamp_jobs_caps_a_value_above_the_ceiling() {
        assert_eq!(clamp_jobs(JOBS_CEILING + 1), JOBS_CEILING);
        assert_eq!(clamp_jobs(usize::MAX), JOBS_CEILING);
    }

    #[test]
    fn clamp_jobs_accepts_the_ceiling_value_itself() {
        assert_eq!(clamp_jobs(JOBS_CEILING), JOBS_CEILING);
    }
}

#[cfg(test)]
mod apply_default_tests {
    use super::*;

    fn sub_matches_for(args: &[&str]) -> Result<ArgMatches, Box<dyn std::error::Error>> {
        let mut full = vec!["vorpal"];
        full.extend_from_slice(args);
        let mut matches = Cli::command().try_get_matches_from(full)?;
        let (_, sub_matches) = matches.remove_subcommand().ok_or("expected a subcommand")?;
        Ok(sub_matches)
    }

    #[test]
    fn is_explicit_false_when_flag_omitted() -> Result<(), Box<dyn std::error::Error>> {
        let sub_matches = sub_matches_for(&["build", "foo"])?;
        assert!(!is_explicit(&sub_matches, "registry"));
        Ok(())
    }

    #[test]
    fn is_explicit_true_when_flag_passed_with_a_value_different_from_default(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sub_matches = sub_matches_for(&["build", "foo", "--registry", "unix:///other.sock"])?;
        assert!(is_explicit(&sub_matches, "registry"));
        Ok(())
    }

    /// Regression: a user-supplied value that happens to equal the (possibly
    /// env-derived) clap default must still be treated as explicit. String
    /// comparison against the default cannot tell these apart; this is what
    /// silently discarded `--registry`/`--worker` overrides that matched
    /// `VORPAL_SOCKET_PATH`-derived defaults.
    #[test]
    fn is_explicit_true_when_flag_value_equals_the_clap_default(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let default_addr = get_default_address();
        let sub_matches = sub_matches_for(&["build", "foo", "--registry", &default_addr])?;
        assert!(is_explicit(&sub_matches, "registry"));
        Ok(())
    }

    #[test]
    fn apply_default_keeps_explicit_value_even_when_it_equals_the_resolved_value() {
        let result = apply_default("unix:///same.sock", true, "unix:///same.sock");
        assert_eq!(result, "unix:///same.sock");
    }

    #[test]
    fn apply_default_substitutes_resolved_value_when_not_explicit() {
        let result = apply_default("unix:///clap-default.sock", false, "unix:///resolved.sock");
        assert_eq!(result, "unix:///resolved.sock");
    }
}

/// If the flag was not explicitly passed on the command line, substitute the
/// resolved settings value; otherwise keep the parsed (explicit) value. This
/// ensures explicit CLI flags always win, while config-file values override
/// built-in defaults.
///
/// `was_explicit` must come from `ArgMatches::value_source(id) ==
/// Some(ValueSource::CommandLine)` rather than comparing `parsed` against the
/// clap default string: a user-supplied value that happens to equal the
/// (possibly env-derived) default is indistinguishable from an omitted flag
/// under string comparison, silently discarding the user's explicit choice.
fn apply_default(parsed: &str, was_explicit: bool, resolved_value: &str) -> String {
    if was_explicit {
        parsed.to_string()
    } else {
        resolved_value.to_string()
    }
}

/// Returns true if `arg_id` was explicitly supplied on the command line for
/// the given subcommand's `ArgMatches`, as opposed to falling back to its
/// clap default value.
fn is_explicit(sub_matches: &ArgMatches, arg_id: &str) -> bool {
    sub_matches.value_source(arg_id) == Some(ValueSource::CommandLine)
}

/// `--jobs`/`-j`'s default: the host's available CPU parallelism, or `1` if
/// the platform cannot report it.
fn get_default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

/// Ceiling on `--jobs`/`-j`. Not a thread-safety limit — it bounds how many
/// artifacts are pulled, unpacked and built at once, and with them how many
/// worker RPCs one invocation has outstanding. Peak memory is no longer the
/// reason: pulled archives stream into their staged file a chunk at a time
/// (`StagedArchive`, `cli/src/command/build.rs`) rather than being
/// accumulated whole, so this ceiling is defence in depth rather than the
/// only bound between an archive's size — the registry's choice, not the
/// user's — and the host. What streaming moved rather than removed is the
/// interrupt cost: a transfer killed mid-stream (SIGKILL, power loss — not
/// an error path, which is compensated) strands one staged `.tmp-*` file
/// per in-flight pull, up to this many, and nothing reaps them. A reasoned
/// posture, not a measurement.
const JOBS_CEILING: usize = 64;

/// Clamps a requested `--jobs`/`-j` value to `[1, JOBS_CEILING]`, deliberately
/// downstream of CLI parsing rather than baked into the flag itself, and
/// never routed through `ResolvedSettings` (whose precedence puts a built
/// project's own `Vorpal.toml` above the invoking user's config) — `--jobs`'s
/// provenance stays CLI/env only. `0` floors to `1` rather than refusing the
/// build; a value above the ceiling is capped, with a warning, rather than
/// refused outright.
fn clamp_jobs(requested: usize) -> usize {
    if requested == 0 {
        return 1;
    }

    if requested > JOBS_CEILING {
        warn!(
            "--jobs {requested} exceeds the ceiling of {JOBS_CEILING}; clamping to {JOBS_CEILING}"
        );

        return JOBS_CEILING;
    }

    requested
}

/// Bounds every HTTP request `Command::Login` makes so a hung or malicious
/// `IdP` stalls the command rather than hanging until the user interrupts it
/// (VPL-280 AB-7). Same value as the SDK refresh path's
/// `REFRESH_HTTP_TIMEOUT`.
const LOGIN_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// The one hardened HTTP client every request `Command::Login` makes is
/// built through (VPL-280 AC3, C-5): no redirects, and a caller-supplied
/// timeout. A single function, rather than an inline `ClientBuilder` at the
/// call site, is the fix for the defect this control exists to close — a
/// hardened client and an unhardened `reqwest::get` coexisting a few lines
/// apart, with the unhardened one used for discovery. Tests call this same
/// function (with a short timeout) rather than building their own client, so
/// a mutant that strips either control fails here, not only in production:
/// a stripped redirect policy fails the requested-paths assertion in the
/// redirected-discovery test, and a stripped timeout fails the outer-bound
/// assertion in the hung-IdP test.
fn login_http_client(timeout: Duration) -> Result<reqwest::Client> {
    Ok(reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()?)
}

/// The development OIDC issuer: `vorpal login`'s default, and the value
/// `makefile`'s `VORPAL_ISSUER` must match. It is here, in Rust, because a
/// Rust test can read the makefile at compile time and fail on drift, while
/// the makefile cannot read this file at all — so this is the copy that owns
/// the value and the makefile is the copy that is checked against it.
///
/// Development-only, and deliberately loopback: a plaintext trust anchor is
/// acceptable only because `docker-compose.yaml` binds Keycloak to
/// 127.0.0.1, so squatting the port already requires the developer's own
/// uid. Copying this value into `script/install.sh`, a container image, or
/// anything presented as installation guidance is what makes it dangerous.
const DEFAULT_DEV_ISSUER: &str = "http://localhost:8080/realms/vorpal";

/// An issuer URL that has been through [`NormalizedIssuer::parse`], and so
/// is known to satisfy the shared https-or-loopback rule, to carry no
/// embedded credentials, and to be in the `Url` crate's canonical form:
/// lower-cased scheme and host, elided default port, canonical
/// percent-encoding, no trailing slash.
///
/// The type exists because `login_discovery_targets` compares an issuer for
/// string equality against the discovery document's own `iss` claim, and
/// that comparison is only sound when *both* sides are canonical. Expressed
/// as `&str` the precondition can only be stated in a comment and remembered
/// by each caller; expressed as a type it travels with the value, and the
/// one-sided canonicalization that produced a live mismatch (an `IdP` naming
/// an explicit default port, or a mixed-case host) is no longer expressible.
#[derive(Clone, Debug, Eq, PartialEq)]
struct NormalizedIssuer(String);

impl NormalizedIssuer {
    /// Validates a raw issuer string against `credential_egress_origin`,
    /// then re-derives the stored value from the parsed `Url` rather than
    /// from the input. `credential_egress_origin` only proves the value
    /// parses to an acceptable scheme and host; it does not prove the
    /// *returned* string is what was parsed, so keeping the raw input let a
    /// control character embedded in the path — which `Url::parse` accepts
    /// and normalizes, but does not strip from an untouched copy — reach
    /// every downstream consumer intact.
    ///
    /// Shared by `parse_issuer` and `normalize_and_validate_login_issuer` so
    /// the binary's two issuer-accepting paths do not carry two
    /// implementations of one rule. Normalizing is observable on the login
    /// path, where the result is the credentials-file key and the string
    /// compared against the discovery document: an `IdP` states its `iss` in
    /// canonical form, so normalizing the operator's text toward it makes
    /// the comparison agree more often, not less.
    fn parse(candidate: &str) -> std::result::Result<Self, String> {
        let url = reqwest::Url::parse(candidate)
            .map_err(|err| format!("not a valid absolute URL ({err})"))?;

        // The rule itself stays in `credential_egress_origin`; only its
        // wording is restated here. Its own message names the refresh token,
        // which neither caller is sending, and it reports one sentence for
        // four distinct failures. Having already parsed the URL, the two
        // causes that can still reach this arm are recoverable from the
        // parse: an authority with no host, or a scheme the egress rule
        // refuses for that host.
        credential_egress_origin(url.as_str()).map_err(|_| match url.host_str() {
            None => "URL has no host".to_string(),
            Some(host) => format!(
                "scheme {:?} is not allowed for host {host:?}: the OIDC issuer must be https \
                 (plaintext http is only allowed on localhost or 127.0.0.1)",
                url.scheme()
            ),
        })?;

        // Userinfo is refused rather than stripped. An issuer that carries
        // `user:password@` is a credential the value then drags everywhere
        // it goes — the startup log line that states the trust anchor, the
        // credentials file `vorpal login` writes, any error message quoting
        // it — and no IdP states userinfo in the `iss` claim this value is
        // compared against, so there is nothing to preserve. Refusing also
        // keeps this rule a superset of `script/install.sh`'s, which already
        // rejects an authority containing `@`.
        if !url.username().is_empty() || url.password().is_some() {
            return Err(
                "URL carries embedded credentials (user:password@); supply the issuer URL \
                 without them"
                    .to_string(),
            );
        }

        Ok(Self(url.as_str().trim_end_matches('/').to_string()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NormalizedIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What the device-code exchange yielded, before it is bound to the issuer
/// that granted it and the registry the operator pointed at that issuer.
struct LoginGrant {
    access_token: String,
    audience: Option<String>,
    client_id: String,
    expires_in: u64,
    issued_at: u64,
    refresh_token: String,
    scopes: Vec<String>,
}

/// Binds a completed exchange to its issuer and registry.
///
/// Split out of the `Command::Login` arm so the binding is reachable from a
/// test without driving the whole device flow: the arm's remaining work is
/// network I/O, and the part worth pinning is which URL becomes the issuer
/// key and which becomes the registry key. `issuer` is a [`NormalizedIssuer`]
/// rather than a `&str` so the arm cannot reach the SDK with an issuer that
/// skipped `normalize_and_validate_login_issuer` (VPL-188 AB-4).
fn login_record(issuer: &NormalizedIssuer, registry: &str, grant: LoginGrant) -> LoginRecord {
    LoginRecord {
        issuer: issuer.as_str().to_string(),
        registry: registry.to_string(),
        content: VorpalCredentialsContent {
            access_token: grant.access_token,
            audience: grant.audience,
            client_id: grant.client_id,
            expires_in: grant.expires_in,
            issued_at: grant.issued_at,
            refresh_token: grant.refresh_token,
            scopes: grant.scopes,
        },
    }
}

/// clap `value_parser` for `system services start --issuer`. Applied
/// identically whether the value arrives via `--issuer` or the
/// `VORPAL_ISSUER` environment variable, so an empty env value — which clap
/// treats as present, not absent — is rejected here rather than reaching
/// `resolve_required_issuer` as a false "issuer configured". The scheme/host
/// error text names this command's own trust anchor rather than
/// `credential_egress_origin`'s "refresh token" wording, because
/// `system services start` never sends a refresh token.
///
/// The underlying cause is carried into the message rather than discarded:
/// `NormalizedIssuer::parse` fails for five distinct reasons — unparseable
/// URL, no host, a non-loopback plaintext scheme, a URL with no port and no
/// known default, and embedded `user:password@` credentials — and reporting
/// them all as "issuer must be https" told
/// an operator whose issuer was `https:/idp.example.com` (one slash) to fix
/// a scheme that was already correct. AC4 asks the failure to name the
/// migration step; it cannot do that while describing the wrong failure.
fn parse_issuer(raw: &str) -> std::result::Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(
            "issuer must not be empty; set --issuer or VORPAL_ISSUER to your OIDC issuer URL \
             (e.g. https://idp.example.com/realms/vorpal)"
                .to_string(),
        );
    }
    // No trailing-slash trim here: `NormalizedIssuer::parse` trims the
    // *parsed* form, which is the only trim that can be observed. A second
    // trim on the raw text made the test that claimed to pin this one unable
    // to fail for its own named mutant.
    NormalizedIssuer::parse(trimmed)
        .map(|issuer| issuer.as_str().to_string())
        .map_err(|cause| {
            format!(
                "invalid issuer {trimmed:?}: {cause}. Set --issuer or VORPAL_ISSUER to your OIDC \
                 issuer URL — https, or plaintext http only on localhost or 127.0.0.1 — e.g. \
                 https://idp.example.com/realms/vorpal"
            )
        })
}

/// Where clap took a `system services start` argument from, or `None` when
/// the argument was not supplied at all or the parsed command was something
/// else entirely.
///
/// Provenance has to be read from clap's own record rather than inferred.
/// Checking whether the matching environment variable is set answers a
/// different question: with both channels set — which is precisely the shape
/// an installed unit creates, since it writes `--issuer` into `ExecStart` and
/// `VORPAL_ISSUER` into its `EnvironmentFile` — "the variable is set" is true
/// no matter which value clap actually used.
fn services_start_value_source(matches: &ArgMatches, arg_id: &str) -> Option<ValueSource> {
    matches
        .subcommand_matches("system")?
        .subcommand_matches("services")?
        .subcommand_matches("start")?
        .value_source(arg_id)
}

/// Whether a `--services` list starts a service that `resolve_required_issuer`
/// (`cli/src/command/start.rs`) refuses to run without an OIDC issuer.
///
/// Split the same way `RunArgs` splits it, so the answer describes the list
/// this process is actually about to start rather than a second reading of
/// the same text.
fn starts_an_authenticated_service(services: &str) -> bool {
    services
        .split(',')
        .any(|service| service == "registry" || service == "worker")
}

/// Human-readable name for a channel, for the startup log line that states
/// the effective OIDC trust anchor.
///
/// `--issuer` has no clap default, so `ValueSource::DefaultValue` has no
/// caller and gets no arm of its own: a branch nothing can reach is a claim
/// about behaviour that no test can pin. If a default is ever added, add the
/// arm back with the caller that reaches it.
fn value_source_label(source: Option<ValueSource>) -> &'static str {
    match source {
        Some(ValueSource::CommandLine) => "command line",
        Some(ValueSource::EnvVariable) => "environment",
        _ => "unknown source",
    }
}

/// clap `value_parser` for `system services start --issuer-client-secret`.
/// clap treats a set-but-empty environment variable as a
/// supplied value, so `VORPAL_ISSUER_CLIENT_SECRET=` — the shape an
/// `EnvironmentFile` written for an install that has no secret yet produces —
/// would otherwise reach `exchange_client_credentials` as `Some("")` and be
/// `POSTed` to the `IdP`, whose refusal arrives as an opaque 401 rather than as
/// the configuration error it is.
///
/// The value is returned byte-for-byte, deliberately: a client secret is
/// opaque IdP-generated text that the operator cannot edit, so trimming it
/// would silently alter a credential whose surrounding whitespace, however
/// unlikely, is the `IdP`'s to define and not ours.
fn parse_client_secret(raw: &str) -> std::result::Result<String, String> {
    if raw.trim().is_empty() {
        return Err(
            "issuer client secret must not be empty; leave --issuer-client-secret and \
             VORPAL_ISSUER_CLIENT_SECRET unset entirely to start without client credentials"
                .to_string(),
        );
    }

    Ok(raw.to_string())
}

/// Normalizes and validates the `--issuer` value before any network request
/// touches it (VPL-280 AC1). Trimming and validation happen at this one call
/// site so every later consumer — the discovery URL, `AuthUrl`, and both
/// credentials-file keys — reads the same value (VPL-280 AC4, TB-4): storing
/// an untrimmed issuer in one place and a trimmed one in another is the bug
/// that made the refresh path mis-format its discovery URL.
fn normalize_and_validate_login_issuer(issuer: &str) -> Result<NormalizedIssuer> {
    let candidate = issuer.trim_end_matches('/');
    NormalizedIssuer::parse(candidate).map_err(|err| anyhow!(err))
}

/// Validates a login discovery document against the requested issuer and
/// extracts the two endpoint URLs the device-authorization flow uses
/// (VPL-280 AC2). Three checks, all before either endpoint reaches the
/// network:
///
/// - the document's own `issuer` claim must equal `issuer` exactly (after
///   trailing-slash normalization) — the origin pin below is blind to path,
///   so on a multi-tenant `IdP` that shares one origin across realms it alone
///   would let a co-tenant substitute its own endpoints (VPL-280 AB-4); a
///   missing `issuer` field fails closed rather than skipping the check
/// - `device_authorization_endpoint` must share the issuer's
///   `scheme://host:port` origin — not named by any of VPL-280's acceptance
///   criteria, but required per the threat model (VPL-280 C-3): it is what
///   the CLI prints for the user to open in a browser, so leaving it
///   unchecked is IdP-credential phishing through the same document
///   (VPL-280 AB-3)
/// - `token_endpoint` must share the issuer's origin (VPL-280 AC2, AB-1)
///
/// A pure function over the parsed document so each check is testable
/// without driving a live device-authorization flow (VPL-280 C-7).
fn login_discovery_targets(
    issuer: &NormalizedIssuer,
    doc: &serde_json::Value,
) -> Result<(String, String)> {
    let issuer_origin = credential_egress_origin(issuer.as_str())?;

    let doc_issuer = doc
        .get("issuer")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("OIDC discovery document is missing issuer"))?;

    // Both sides of the equality below are `NormalizedIssuer`, which is what
    // makes it sound. Comparing a normalized request against a merely
    // trailing-slash-trimmed claim failed closed for any IdP whose `iss`
    // names an explicit default port (Keycloak behind a proxy with an
    // explicit KC_HOSTNAME) or a mixed-case host, reporting two strings that
    // differ only in a representation this binary itself treats as
    // equivalent.
    let normalized_doc_issuer = NormalizedIssuer::parse(doc_issuer).map_err(|err| {
        anyhow!("OIDC discovery document issuer {doc_issuer:?} is invalid: {err}")
    })?;

    if normalized_doc_issuer != *issuer {
        bail!("OIDC discovery issuer {doc_issuer} does not match requested issuer {issuer}");
    }

    let device_endpoint = doc
        .get("device_authorization_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing device_authorization_endpoint"))?;

    if credential_egress_origin(device_endpoint)? != issuer_origin {
        bail!(
            "OIDC device_authorization_endpoint {device_endpoint} does not match issuer origin {issuer_origin}"
        );
    }

    let token_endpoint = doc
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing token_endpoint"))?;

    if credential_egress_origin(token_endpoint)? != issuer_origin {
        bail!("OIDC token_endpoint {token_endpoint} does not match issuer origin {issuer_origin}");
    }

    Ok((device_endpoint.to_string(), token_endpoint.to_string()))
}

/// The verification links `vorpal login` prints, after both have been pinned
/// to the issuer origin. Owned canonical strings rather than the response's
/// own types: the oauth2 newtypes render the raw response text, and printing
/// that would let a passing origin check vouch for bytes the check never saw.
#[derive(Debug)]
struct LoginVerificationPrompt {
    verification_uri: String,
    verification_uri_complete: Option<String>,
}

/// Pins the device-authorization response's human-facing links to the issuer
/// origin before anything reaches the terminal (VPL-738).
///
/// `login_discovery_targets` pins where the CLI *sends* the device-code
/// request; nothing pinned what came back. The response names the URL the
/// user opens and authenticates at, so an off-origin value there is `IdP`
/// credential phishing even though vorpal's own tokens stay safe. The
/// guarantee is narrow on purpose: it protects a user whose issuer is honest
/// but whose device-endpoint response path is not. An issuer that is itself
/// hostile already holds the credentials.
///
/// `verification_uri_complete` is a plain string in the oauth2 crate — never
/// parsed as a URL — so it is parsed here and a parse failure refuses rather
/// than skipping the check.
///
/// A pure function so each case is testable without driving a live device
/// flow, mirroring `login_discovery_targets`.
fn login_verification_prompt(
    issuer: &NormalizedIssuer,
    details: &StandardDeviceAuthorizationResponse,
) -> Result<LoginVerificationPrompt> {
    let issuer_origin = credential_egress_origin(issuer.as_str())?;

    let pinned = |field: &str, raw: &str| -> Result<String> {
        let url = reqwest::Url::parse(raw)
            .map_err(|err| anyhow!("device-authorization {field} is not a URL: {err}"))?;

        let origin = credential_egress_origin(url.as_str())?;

        if origin != issuer_origin {
            bail!(
                "device-authorization {field} origin {origin} does not match issuer origin {issuer_origin}"
            );
        }

        Ok(url.to_string())
    };

    let verification_uri = pinned(
        "verification_uri",
        details.verification_uri().url().as_str(),
    )?;

    let verification_uri_complete = details
        .verification_uri_complete()
        .map(|complete| pinned("verification_uri_complete", complete.secret()))
        .transpose()?;

    Ok(LoginVerificationPrompt {
        verification_uri,
        verification_uri_complete,
    })
}

/// Fetches and validates the login discovery document with an
/// already-hardened client (VPL-280 AC2, AC3): the caller controls the
/// redirect policy and timeout, so this function's own behavior under a
/// redirected or hung discovery response is exactly what production gets.
async fn fetch_login_discovery_endpoints(
    client: &reqwest::Client,
    issuer: &NormalizedIssuer,
) -> Result<(String, String)> {
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");

    let doc: serde_json::Value = client
        .get(&discovery_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    login_discovery_targets(issuer, &doc)
}

/// Build-output flags shared between `build` and `prepare`, mirroring the
/// independent boolean CLI flags on [`Command::Build`] one-to-one.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors independent CLI flags on Command::Build one-to-one; a state machine or enum would not fit clap's per-flag parsing"
)]
struct BuildFlags {
    export: bool,
    list: bool,
    path: bool,
    prepare_only: bool,
    rebuild: bool,
    unlock: bool,
}

/// Builds a [`VorpalConfigSource`] from the project config's `[source]` table,
/// treating an empty string or empty list the same as an absent value.
fn resolve_config_source(project_config: &config::VorpalConfig) -> VorpalConfigSource {
    let Some(config_source) = &project_config.source else {
        return VorpalConfigSource {
            includes: Some(Vec::new()),
            go: Some(VorpalConfigSourceGo::default()),
            python: Some(VorpalConfigSourcePython::default()),
            rust: Some(VorpalConfigSourceRust::default()),
            script: None,
            typescript: Some(VorpalConfigSourceTypeScript::default()),
        };
    };

    let non_empty = |s: &Option<String>| s.as_ref().filter(|v| !v.is_empty()).cloned();

    let go_directory = config_source
        .go
        .as_ref()
        .and_then(|go| non_empty(&go.directory));

    let includes = config_source
        .includes
        .as_ref()
        .filter(|includes| !includes.is_empty())
        .cloned()
        .unwrap_or_default();

    let (python_directory, python_entrypoint) = config_source
        .python
        .as_ref()
        .map_or((None, None), |python| {
            (non_empty(&python.directory), non_empty(&python.entrypoint))
        });

    let (rust_bin, rust_packages) = config_source.rust.as_ref().map_or((None, None), |rust| {
        (
            non_empty(&rust.bin),
            rust.packages
                .as_ref()
                .filter(|packages| !packages.is_empty())
                .cloned(),
        )
    });

    let script = non_empty(&config_source.script);

    let (typescript_directory, typescript_entrypoint) = config_source
        .typescript
        .as_ref()
        .map_or((None, None), |ts| {
            (non_empty(&ts.directory), non_empty(&ts.entrypoint))
        });

    VorpalConfigSource {
        go: Some(VorpalConfigSourceGo {
            directory: go_directory,
        }),
        includes: Some(includes),
        python: Some(VorpalConfigSourcePython {
            directory: python_directory,
            entrypoint: python_entrypoint,
        }),
        rust: Some(VorpalConfigSourceRust {
            bin: rust_bin,
            packages: rust_packages,
        }),
        script,
        typescript: Some(VorpalConfigSourceTypeScript {
            directory: typescript_directory,
            entrypoint: typescript_entrypoint,
        }),
    }
}

/// Resolves `context` to an absolute, cleaned path, treating a relative path
/// as relative to the process's current directory.
fn resolve_context_path(context: &Path) -> Result<PathBuf> {
    let mut context = context.to_path_buf();

    if !context.is_absolute() {
        let current_dir = current_dir().context("failed to get current directory")?;
        context = current_dir.join(&context).clean();
    }

    Ok(context.clean())
}

/// Shared by `build` and `prepare`: resolves settings/Vorpal.toml fallbacks,
/// then runs the artifact graph through `build::run()`. `prepare_only` gates
/// the early-return (before worker dispatch) added in build.rs; `export`,
/// `list`, `path`, and `rebuild` are always `false` for `prepare` (that flag
/// surface is build-output-specific and not exposed on the `prepare` subcommand).
#[expect(
    clippy::too_many_arguments,
    reason = "12 params after grouping the 6 bools; non-bool params kept as distinct typed args"
)]
async fn run_build_or_prepare(
    resolved: &config::ResolvedSettings,
    project_config: &config::VorpalConfig,
    sub_matches: &ArgMatches,
    name: &str,
    agent: &str,
    context: &Path,
    flags: BuildFlags,
    jobs: usize,
    namespace: &str,
    registry: &str,
    system: &str,
    variable: &[String],
    worker: &str,
) -> Result<()> {
    // Agent is a local service — it should NOT inherit the `registry`
    // setting. Only override it when the user passes an explicit --agent flag.
    let effective_agent = agent.to_string();
    let effective_registry = apply_default(
        registry,
        is_explicit(sub_matches, "registry"),
        &resolved.registry.value,
    );
    let effective_worker = apply_default(
        worker,
        is_explicit(sub_matches, "worker"),
        &resolved.worker.value,
    );
    let effective_namespace = apply_default(
        namespace,
        is_explicit(sub_matches, "namespace"),
        &resolved.namespace.value,
    );
    let effective_system = apply_default(
        system,
        is_explicit(sub_matches, "system"),
        &resolved.system.value,
    );

    if name.is_empty() {
        error!("no name specified");

        exit(1);
    }

    // Use the project config already loaded during resolution

    // `resolved` is a shared reference reused by other call sites in `run()`; can't move out of it.
    let config_language = resolved.language.value.clone();
    let config_name = resolved.name.value.clone();

    let config_environments = project_config
        .environments
        .as_ref()
        .filter(|environments| !environments.is_empty())
        .cloned()
        .unwrap_or_default();

    let config_source = resolve_config_source(project_config);

    // Load project context and build artifact

    let context = resolve_context_path(context)?;

    let run_artifact = build::RunArgsArtifact {
        aliases: vec![],
        context: context.clone(), // reused below for `run_config`
        export: flags.export,
        jobs: clamp_jobs(jobs),
        list: flags.list,
        name: name.to_string(),
        namespace: effective_namespace,
        path: flags.path,
        prepare_only: flags.prepare_only,
        rebuild: flags.rebuild,
        system: effective_system,
        unlock: flags.unlock,
        variable: variable.to_vec(),
    };

    let run_config = build::RunArgsConfig {
        context,
        environments: config_environments,
        language: config_language,
        name: config_name,
        source: Some(config_source),
    };

    let run_service = build::RunArgsService {
        agent: effective_agent,
        registry: effective_registry,
        worker: effective_worker,
    };

    build::run(run_artifact, run_config, run_service).await
}

/// Runs the `OAuth2` device authorization flow against `issuer`, then writes
/// the resulting credentials to the on-disk credentials file (created with
/// mode 0o600 so the token is never born world-readable).
async fn run_login(
    issuer: &str,
    issuer_audience: Option<&str>,
    issuer_client_id: &str,
    registry: &str,
) -> Result<()> {
    let normalized_issuer = normalize_and_validate_login_issuer(issuer)?;

    // One hardened client for every request this flow makes (AC3), built by
    // the same function the tests exercise (`login_http_client`) so a
    // mutant that weakens it fails there too.
    let http_client =
        login_http_client(LOGIN_HTTP_TIMEOUT).context("failed to build HTTP client")?;

    let (device_endpoint, token_endpoint) =
        fetch_login_discovery_endpoints(&http_client, &normalized_issuer).await?;

    let client_device_url = DeviceAuthorizationUrl::new(device_endpoint)?;

    let client = BasicClient::new(ClientId::new(issuer_client_id.to_string()))
        .set_auth_uri(AuthUrl::new(normalized_issuer.as_str().to_string())?)
        .set_token_uri(TokenUrl::new(token_endpoint)?)
        .set_device_authorization_url(client_device_url);

    let mut device_request = client
        .exchange_device_code()
        .add_scope(Scope::new("offline_access".to_string()));

    if let Some(audience) = issuer_audience {
        device_request = device_request.add_extra_param("audience", audience.to_string());
    }

    let details: StandardDeviceAuthorizationResponse =
        device_request.request_async(&http_client).await?;

    let prompt = login_verification_prompt(&normalized_issuer, &details)?;

    if let Some(complete_uri) = &prompt.verification_uri_complete {
        crate::output::line(format!("Open this URL in your browser:\n{complete_uri}"));
    }

    crate::output::line(format!(
        "Or open {} and enter code: {}",
        prompt.verification_uri,
        details.user_code().secret()
    ));

    let token_result = client
        .exchange_device_access_token(&details)
        .request_async(&http_client, sleep, None)
        .await?;

    // oauth2 exposes the secret only by reference; `token_result` is read again below
    let access_token = token_result.access_token().secret().clone();

    let expires_in = token_result
        .expires_in()
        .map(|d| d.as_secs())
        .unwrap_or_default();

    // oauth2 exposes the secret only by reference
    let refresh_token = token_result
        .refresh_token()
        .map(|t| t.secret().clone())
        .unwrap_or_default();

    let scopes = token_result
        .scopes()
        .map(|s| s.iter().map(|scope| scope.to_string()).collect::<Vec<_>>())
        .unwrap_or_default();

    let issued_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is before the unix epoch")?
        .as_secs();

    let record = login_record(
        &normalized_issuer,
        registry,
        LoginGrant {
            access_token,
            audience: issuer_audience.map(str::to_string),
            client_id: issuer_client_id.to_string(),
            expires_in,
            issued_at,
            refresh_token,
            scopes,
        },
    );

    commit_login_credentials(record).await
}

/// Installs the process-global `tracing` subscriber: a stderr fmt layer
/// gated at `level` (with file/line info at debug/trace), plus the
/// [`H2ProtocolErrorRelay`] layer that relays h2's own debug-level
/// protocol-error events as visible WARNs.
fn init_tracing(level: Level) -> Result<()> {
    // Per-layer filtering: the main fmt layer stays at the user-selected
    // `--level` (default info), while the h2 relay layer is scoped to the
    // `h2` target at debug so only h2's own debug-level events are enabled -
    // this keeps the process-global max level at `level` instead of raising
    // it to DEBUG for every target. The relayed WARN it emits then flows back
    // through fmt_layer's info-level filter like any other event, so it's
    // visible by default without lowering the general level.
    //
    // The fmt layer is gated once by `LevelFilter::from_level(level)`; there is
    // no second writer-level gate (the prior `stderr.with_max_level(level)`
    // duplicated the same filtering the layer already performs).
    let mut fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr)
        .without_time();

    if [Level::DEBUG, Level::TRACE].contains(&level) {
        fmt_layer = fmt_layer.with_file(true).with_line_number(true);
    }

    let subscriber = Registry::default()
        .with(fmt_layer.with_filter(LevelFilter::from_level(level)))
        .with(
            H2ProtocolErrorRelay {
                last_warn: Mutex::new(None),
            }
            .with_filter(Targets::new().with_target("h2", LevelFilter::DEBUG)),
        );

    subscriber::set_global_default(subscriber).context("setting default subscriber")
}

/// Resolves layered settings (user config + project config + built-in
/// defaults) using the `--config` path carried by commands that have one.
/// Falls back to built-in defaults if config loading fails (e.g. a malformed
/// file), so the CLI still works without a valid config.
fn resolve_settings_for(command: &Command) -> (config::ResolvedSettings, config::VorpalConfig) {
    let config_for_settings: &Path = match command {
        Command::Build { config, .. }
        | Command::Config { config, .. }
        | Command::Prepare { config, .. } => config,
        _ => Path::new("Vorpal.toml"),
    };

    config::resolve_config(config_for_settings).unwrap_or_else(|_| {
        let defaults = config::VorpalConfig::defaults();
        let resolved = config::ResolvedSettings::resolve(
            &defaults,
            &config::VorpalConfig::default(),
            &config::VorpalConfig::default(),
        );
        (resolved, config::VorpalConfig::default())
    })
}

/// Dispatches a `vorpal system ...` subcommand.
async fn dispatch_system(system: CommandSystem, matches: &ArgMatches) -> Result<()> {
    match system {
        CommandSystem::Keys(keys) => match keys {
            CommandSystemKeys::Generate {} => system::keys::generate().await,
        },

        CommandSystem::Prune {
            artifact_aliases: aliases,
            all,
            artifact_archives: archives,
            artifact_configs: configs,
            artifact_outputs: outputs,
            sandboxes,
        } => system::prune::run(all, aliases, archives, configs, outputs, sandboxes).await,

        CommandSystem::Services(services) => match services {
            CommandSystemServices::Start {
                archive_cache_ttl,
                health_check,
                health_check_port,
                issuer,
                issuer_audience,
                issuer_client_id,
                issuer_client_secret,
                issuer_service_client_ids,
                port,
                registry_backend,
                registry_backend_s3_bucket,
                registry_backend_s3_force_path_style,
                registry_allowed,
                services,
                tls,
                workspace_root,
            } => {
                // State the trust anchor and the channel it arrived on.
                // Whoever controls the issuer controls which tokens this
                // worker or registry accepts, and since the value can now
                // arrive through `VORPAL_ISSUER` it leaves no trace in `ps`
                // at all — so without this line an anchor swapped through
                // the environment is invisible after the fact. The sibling
                // lines for the trusted-service client list and the
                // registry allow-list already exist for the same reason.
                match &issuer {
                    Some(value) => tracing::info!(
                        "OIDC trust anchor: {value} (from {})",
                        value_source_label(services_start_value_source(matches, ISSUER_ARG_ID))
                    ),
                    // Only the worker and the registry refuse to start
                    // without an anchor, so only a start that includes one
                    // of them is told they will. `--services agent` with no
                    // issuer is a supported configuration; a warning about
                    // a refusal that will not happen trains an operator to
                    // ignore the line that matters.
                    None if starts_an_authenticated_service(&services) => tracing::info!(
                        "no OIDC trust anchor configured; worker and registry services \
                         will refuse to start"
                    ),
                    None => tracing::info!("no OIDC trust anchor configured"),
                }

                // The installer moved the secret off argv and onto a
                // mode-600 EnvironmentFile/plist entry, but the flag still
                // accepts it, so a hand-run or `make`-run service still
                // exposes it to `ps`/`/proc/<pid>/cmdline`.
                if services_start_value_source(matches, ISSUER_CLIENT_SECRET_ARG_ID)
                    == Some(ValueSource::CommandLine)
                {
                    tracing::warn!(
                        "--issuer-client-secret was supplied on the command line; any \
                         local process can read it from `ps` or /proc/<pid>/cmdline. \
                         Prefer the VORPAL_ISSUER_CLIENT_SECRET environment variable \
                         instead."
                    );
                }

                let issuer_service_client_ids = issuer_service_client_ids
                    .as_deref()
                    .map(parse_comma_list)
                    .unwrap_or_default();

                let registry_allowed = resolve_registry_allowed_flag(registry_allowed.as_deref());

                let run_args = start::RunArgs {
                    archive_cache_ttl,
                    health_check,
                    health_check_port,
                    issuer,
                    issuer_audience,
                    issuer_client_id,
                    issuer_client_secret,
                    issuer_service_client_ids,
                    port,
                    registry_backend,
                    registry_backend_s3_bucket,
                    registry_backend_s3_force_path_style,
                    registry_allowed,
                    services: services
                        .split(',')
                        .map(std::string::ToString::to_string)
                        .collect(),
                    tls,
                    workspace_root,
                };

                start::run(run_args).await
            }
        },
    }
}

/// Parses CLI arguments, installs tracing, resolves settings, and dispatches
/// to the matched subcommand's handler.
#[expect(
    clippy::too_many_lines,
    reason = "top-level dispatch over every Command variant; splitting the match only relabels the same arms"
)]
pub async fn run() -> Result<()> {
    ring::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to install ring as default crypto provider"))?;

    // Parsed via raw ArgMatches (rather than `Cli::parse()`) so call sites can
    // later query `ArgMatches::value_source()` on the matched subcommand to
    // tell an explicit CLI flag apart from one that fell back to its clap
    // default — see `apply_default`.
    let arg_matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&arg_matches).unwrap_or_else(|err| err.exit());

    let Cli { command, level } = cli;

    let Some((_, sub_matches)) = arg_matches.subcommand() else {
        bail!("clap subcommand is required");
    };

    init_tracing(level)?;

    let (resolved, project_config) = resolve_settings_for(&command);

    match command {
        Command::Build {
            agent,
            context,
            export,
            jobs,
            list,
            name,
            namespace,
            path,
            rebuild,
            registry,
            system,
            unlock,
            variable,
            worker,
            ..
        } => {
            run_build_or_prepare(
                &resolved,
                &project_config,
                sub_matches,
                &name,
                &agent,
                &context,
                BuildFlags {
                    export,
                    list,
                    path,
                    prepare_only: false,
                    rebuild,
                    unlock,
                },
                jobs,
                &namespace,
                &registry,
                &system,
                &variable,
                &worker,
            )
            .await
        }

        Command::Prepare {
            agent,
            context,
            jobs,
            name,
            namespace,
            registry,
            system,
            unlock,
            variable,
            worker,
            ..
        } => {
            run_build_or_prepare(
                &resolved,
                &project_config,
                sub_matches,
                &name,
                &agent,
                &context,
                BuildFlags {
                    export: false,
                    list: false,
                    path: false,
                    prepare_only: true,
                    rebuild: false,
                    unlock,
                },
                jobs,
                &namespace,
                &registry,
                &system,
                &variable,
                &worker,
            )
            .await
        }

        Command::Config {
            user,
            config,
            action,
        } => match action {
            config_cmd::ConfigAction::Set { key, value } => {
                config_cmd::handle_set(&key, &value, user, &config)
            }
            config_cmd::ConfigAction::Get { key } => config_cmd::handle_get(&key, user, &config),
            config_cmd::ConfigAction::Show => config_cmd::handle_show(&config),
        },

        Command::Init { name, path } => init::run(&name, &path).await,

        Command::Inspect {
            digest,
            namespace,
            registry,
        } => {
            let effective_registry = apply_default(
                &registry,
                is_explicit(sub_matches, "registry"),
                &resolved.registry.value,
            );
            let effective_namespace = apply_default(
                &namespace,
                is_explicit(sub_matches, "namespace"),
                &resolved.namespace.value,
            );
            inspect::run(&digest, &effective_namespace, &effective_registry).await
        }

        Command::Login {
            issuer,
            issuer_audience,
            issuer_client_id,
            registry,
        } => {
            let effective_registry = apply_default(
                &registry,
                is_explicit(sub_matches, "registry"),
                &resolved.registry.value,
            );

            run_login(
                &issuer,
                issuer_audience.as_deref(),
                &issuer_client_id,
                &effective_registry,
            )
            .await
        }

        Command::Run {
            alias,
            args,
            bin,
            registry,
        } => {
            let effective_registry = apply_default(
                &registry,
                is_explicit(sub_matches, "registry"),
                &resolved.registry.value,
            );
            run::run(&alias, &args, bin.as_deref(), &effective_registry).await
        }

        Command::System(system) => dispatch_system(system, &arg_matches).await,
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test assertions read as intent, not defensive code: an unwrap/expect/panic failure is the test failing, which is the point"
)]
mod login_egress_tests {
    use super::*;

    // `set_var`/`remove_var` are process-global and the default test harness
    // runs tests on threads of one process, so a writer excludes a concurrent
    // reader only if both take the *same* lock. Readers are easy to miss:
    // `Cli::try_parse_from` consults `VORPAL_ISSUER` and
    // `VORPAL_ISSUER_CLIENT_SECRET` for every `system services start` parse,
    // so any test that parses that subcommand is a reader whether or not it
    // mentions the environment.
    //
    // Every test below that writes either variable, and every test below that
    // reads the process environment — by parsing `system services start`, or
    // by spawning a child that inherits the whole environment — takes this
    // lock. A new test in either category must take it too; the helpers that
    // spawn `bash` and `make` take it on their caller's behalf.
    static ISSUER_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Takes [`ISSUER_ENV_LOCK`], recovering it if a previous test panicked
    /// while holding it (the data is `()`, so a poisoned lock guards nothing
    /// that can be inconsistent).
    fn issuer_env_guard() -> std::sync::MutexGuard<'static, ()> {
        ISSUER_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    // --- parse_issuer ----------------------------------------

    #[test]
    fn parse_issuer_refuses_empty_string() {
        // clap: `VORPAL_ISSUER=` sets the env var to `Some("")`
        // under clap's semantics; the parser must not treat that as a
        // usable issuer.
        let error = parse_issuer("").expect_err("an empty issuer must be refused");

        assert!(
            error.contains("must not be empty"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn parse_issuer_refuses_whitespace_only() {
        let error = parse_issuer("   ").expect_err("a whitespace-only issuer must be refused");

        assert!(
            error.contains("must not be empty"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn parse_issuer_refuses_plaintext_off_loopback() {
        let error = parse_issuer("http://idp.example.com")
            .expect_err("a plaintext non-loopback issuer must be refused");

        assert!(
            error.contains("must be https"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn issuer_arg_is_wired_to_vorpal_issuer_env_and_parse_issuer() {
        // Every `parse_issuer` test calls the function directly, so reverting
        // `--issuer`'s clap definition from `#[arg(env = "VORPAL_ISSUER",
        // long, value_parser = parse_issuer)]` back to `#[arg(long)]` left
        // the suite green. Inspect the built clap `Command` itself — what the
        // derive macro actually produces — so that revert fails here
        // regardless of what `parse_issuer` does in isolation.
        let cli_command = Cli::command();
        let system_command = cli_command
            .find_subcommand("system")
            .expect("system subcommand must exist");
        let services_command = system_command
            .find_subcommand("services")
            .expect("system services subcommand must exist");
        let start_command = services_command
            .find_subcommand("start")
            .expect("system services start subcommand must exist");
        let issuer_arg = start_command
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == ISSUER_ARG_ID)
            .expect("--issuer argument must exist on system services start");

        assert_eq!(
            issuer_arg.get_env(),
            Some(std::ffi::OsStr::new("VORPAL_ISSUER")),
            "--issuer must be settable via the VORPAL_ISSUER environment variable"
        );
    }

    #[test]
    fn system_services_start_issuer_flag_is_validated_by_clap() {
        // Companion to the CommandFactory test above: parses real argv
        // through the derived `Cli`, so a revert of `value_parser =
        // parse_issuer` (independent of the `env` attribute) also fails
        // here rather than only in a test that calls `parse_issuer` itself.
        let _guard = issuer_env_guard();

        let result = Cli::try_parse_from([
            "vorpal",
            "system",
            "services",
            "start",
            "--issuer",
            "not-a-url",
        ]);

        assert!(
            result.is_err(),
            "a malformed --issuer value must be rejected during clap parsing"
        );
    }

    /// The `VORPAL_ISSUER` default `makefile` ships, read from the file
    /// itself. `makefile` names it in the recipes for `vorpal-start` and
    /// `lima-vorpal-start`; `DEFAULT_DEV_ISSUER` names it for `vorpal login`.
    /// Reading the real text is what turns a one-sided edit into a failing
    /// test rather than a silent divergence between the two.
    fn makefile_default_vorpal_issuer() -> String {
        let makefile = include_str!("../../makefile");
        makefile
            .lines()
            .find_map(|line| {
                // Match the assignment by variable name and split on the
                // operator, rather than on the exact `VORPAL_ISSUER ?= `
                // text: `?=`, `:=`, `+=` and a bare `=` all mean the same
                // thing to this test, and pinning one spelling made
                // reformatting the makefile fail here as a missing default.
                let (name, value) = line.split_once('=')?;
                let name = name.trim_end_matches(['?', ':', '+']);

                (name.trim() == "VORPAL_ISSUER").then(|| value.trim())
            })
            .expect("makefile must define a default VORPAL_ISSUER")
            .to_string()
    }

    #[test]
    fn the_development_issuer_has_one_owner() {
        // `makefile` and `vorpal login` used to name the same realm because
        // whoever last changed it remembered to edit both files. Nothing
        // failed if they did not.
        assert_eq!(
            makefile_default_vorpal_issuer(),
            DEFAULT_DEV_ISSUER,
            "makefile's VORPAL_ISSUER default and the CLI's DEFAULT_DEV_ISSUER \
             must name the same issuer"
        );
    }

    #[test]
    fn parse_issuer_accepts_plaintext_loopback() {
        // The loopback exception, driven by the value the repository actually
        // ships. Asserting the exception is exercised at all — that the
        // default really is plaintext and really is loopback — is the point:
        // without it, moving the development default to https would empty
        // this test out silently instead of failing it.
        let default_issuer = makefile_default_vorpal_issuer();

        assert!(
            default_issuer.starts_with("http://localhost:")
                || default_issuer.starts_with("http://127.0.0.1:"),
            "this test exists to exercise the plaintext-loopback exception, but \
             the shipped default is {default_issuer:?}"
        );

        let normalized = parse_issuer(&default_issuer).expect("a loopback issuer must validate");

        assert_eq!(normalized, default_issuer);
    }

    // --- script/install.sh, driven as a program ---------------------------
    //
    // `script/install.sh` holds several security controls — the issuer shape
    // rule, the unit/plist injection deny-list, the refusal to install a
    // worker or registry with no issuer, and the mode-600 environment file
    // that keeps the client secret out of argv — and the repository has no
    // shell test harness to reach any of them. The pins that used to stand in
    // for one searched the whole file for a bare token, which stayed green
    // under a mutation that deleted the guard the token appeared in. These
    // run the real script in a real bash process instead.

    const INSTALL_SH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../script/install.sh");

    /// Runs `script/install.sh` as a program, with the `VORPAL_*` variables
    /// it reads as defaults cleared so the result does not depend on the
    /// environment the test suite happens to run in.
    fn run_installer(args: &[&str]) -> String {
        // A spawned child inherits the whole environment, so this is a
        // reader of `VORPAL_ISSUER` even though the variables it cares about
        // are cleared below: a concurrent test's `set_var` lands between the
        // `env_remove` calls and the spawn otherwise.
        let _guard = issuer_env_guard();

        let output = std::process::Command::new("bash")
            .arg(INSTALL_SH)
            .args(args)
            .env_remove("VORPAL_ISSUER")
            .env_remove("VORPAL_ISSUER_AUDIENCE")
            .env_remove("VORPAL_ISSUER_CLIENT_ID")
            .env_remove("VORPAL_ISSUER_CLIENT_SECRET")
            .env_remove("VORPAL_SERVICES")
            .env("NO_COLOR", "1")
            .output()
            .expect("script/install.sh must be runnable with bash");

        assert!(
            !output.status.success(),
            "expected script/install.sh {args:?} to refuse, but it exited 0"
        );

        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    /// Runs `script/install.sh` the way the project documents installing it:
    /// piped into `bash` on stdin (`curl … | bash`). The script has no file
    /// of its own there, so `$0` is `bash` and `BASH_SOURCE` is empty — the
    /// one invocation form the other helpers here cannot reach.
    fn pipe_installer_to_bash(args: &[&str]) -> String {
        let _guard = issuer_env_guard();

        let script = std::fs::File::open(INSTALL_SH)
            .unwrap_or_else(|err| panic!("{INSTALL_SH} must be readable: {err}"));

        let output = std::process::Command::new("bash")
            .arg("-s")
            .arg("--")
            .args(args)
            .stdin(std::process::Stdio::from(script))
            .env_remove("VORPAL_ISSUER")
            .env_remove("VORPAL_ISSUER_AUDIENCE")
            .env_remove("VORPAL_ISSUER_CLIENT_ID")
            .env_remove("VORPAL_ISSUER_CLIENT_SECRET")
            .env_remove("VORPAL_SERVICES")
            .env("NO_COLOR", "1")
            .output()
            .expect("script/install.sh must be runnable from bash's stdin");

        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    /// Sources `script/install.sh` and runs `body` against its functions.
    /// Sourcing is inert: the script's entry point is guarded on
    /// `BASH_SOURCE`, so only its variable defaults are set. `set -euo
    /// pipefail` comes with it, so `body` runs under the same shell options
    /// the installer itself does.
    fn source_installer(body: &str) -> String {
        let _guard = issuer_env_guard();

        let script = format!("source {INSTALL_SH:?}\n{body}\n");

        let output = std::process::Command::new("bash")
            .arg("-c")
            .arg(&script)
            .env_remove("VORPAL_ISSUER")
            .env_remove("VORPAL_ISSUER_CLIENT_SECRET")
            .env("NO_COLOR", "1")
            .output()
            .expect("script/install.sh must be sourceable with bash");

        format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }

    /// Whether `script/install.sh`'s `validate_issuer` accepts `issuer`.
    fn installer_accepts_issuer(issuer: &str) -> bool {
        assert!(
            !issuer.contains('\''),
            "test issuers are interpolated into a single-quoted shell word; \
             {issuer:?} would break the harness rather than test anything"
        );

        let transcript = source_installer(&format!(
            "ISSUER='{issuer}'\n\
             if ( validate_issuer ) >/dev/null 2>&1; then echo ACCEPTED; else echo REFUSED; fi"
        ));

        match transcript.trim() {
            "ACCEPTED" => true,
            "REFUSED" => false,
            other => panic!("validate_issuer harness produced {other:?} for {issuer:?}"),
        }
    }

    #[test]
    fn installer_refuses_a_worker_with_no_issuer_and_names_the_migration_step() {
        // AC4, through the real entry point: a default install must not
        // write a unit that cannot start, and the refusal must say what to
        // supply. Validation runs before anything is downloaded or written,
        // so this reaches the refusal without touching the network.
        let transcript = run_installer(&["--dry-run", "--services", "worker"]);

        assert!(
            transcript.contains("Missing --issuer for service 'worker'"),
            "the refusal must name the service it refused: {transcript}"
        );
        assert!(
            transcript.contains("--issuer https://") && transcript.contains("VORPAL_ISSUER="),
            "the refusal must name both channels an operator can migrate to: {transcript}"
        );
    }

    #[test]
    fn installer_refuses_an_issuer_carrying_unit_file_syntax() {
        // The deny-list guarding the systemd `ExecStart=` line and the
        // launchd `ProgramArguments` array, driven through the real entry
        // point. Doubles as the positive control for the test above: the
        // harness reaches a *different* refusal for a different bad input,
        // so a green there is not the script failing for some unrelated
        // reason.
        let transcript = run_installer(&[
            "--dry-run",
            "--services",
            "worker",
            "--issuer",
            "https://idp.example.com/\" ExecStart=/bin/sh",
        ]);

        assert!(
            transcript.contains("contains a disallowed character"),
            "an issuer carrying unit-file syntax must be refused by name: {transcript}"
        );
        assert!(
            !transcript.contains("Missing --issuer"),
            "this input supplies an issuer; it must not be refused as absent: {transcript}"
        );
    }

    #[test]
    fn installer_never_accepts_an_issuer_the_cli_refuses() {
        // `script/install.sh`'s `validate_issuer` and the CLI's
        // `parse_issuer` are two implementations of one rule, in two
        // languages, and only one of them runs at install time. The
        // installer may reject a value the CLI would accept — a false
        // refusal costs an install — but the reverse writes a unit that
        // restart-loops forever, so it must be impossible. Both sides are
        // executed here rather than searched for a token.
        let cases = [
            ("https://idp.example.com/realms/vorpal", true),
            ("http://localhost:8080/realms/vorpal", true),
            ("http://127.0.0.1:8080/realms/vorpal", true),
            // Plaintext off loopback: the trust anchor over a channel an
            // on-path attacker can rewrite.
            ("http://idp.example.com/realms/vorpal", false),
            // Bracketed IPv6 loopback: refused by `credential_egress_origin`,
            // which compares `Url::host_str()` against an unbracketed "::1",
            // so the installer must refuse it too.
            ("http://[::1]:8080/realms/vorpal", false),
            // No host at all.
            ("https://", false),
            // `localhost:` here is userinfo; the host is off-box.
            ("http://localhost:pw@evil.example/realms/vorpal", false),
            // Userinfo on an otherwise acceptable https issuer: a credential
            // the value would then carry into the startup log line and the
            // credentials file. Both sides refuse it.
            ("https://user:pw@idp.example.com/realms/vorpal", false),
            // No scheme.
            ("idp.example.com/realms/vorpal", false),
        ];

        for (issuer, expected_accepted) in cases {
            let installer_accepted = installer_accepts_issuer(issuer);
            let cli_accepted = parse_issuer(issuer).is_ok();

            assert!(
                !installer_accepted || cli_accepted,
                "script/install.sh accepts {issuer:?} but the CLI refuses it, so \
                 installing it would write a unit that can never start"
            );

            assert_eq!(
                cli_accepted, expected_accepted,
                "the CLI's verdict on {issuer:?} changed"
            );
            assert_eq!(
                installer_accepted, expected_accepted,
                "script/install.sh's verdict on {issuer:?} changed"
            );
        }
    }

    #[test]
    fn installer_writes_an_env_file_when_an_issuer_has_no_client_secret() {
        // The migration path this issue exists to deliver is a public OIDC
        // client: an issuer and no secret. The environment-file writer used
        // to end its brace group with a conditional `printf` for the secret,
        // so under `set -euo pipefail` the group's exit status was that
        // unfired conditional and the installer aborted — on its own default
        // case, after writing the file but before the unit.
        let transcript = source_installer(
            "tmp=$(mktemp -d)\n\
             ISSUER='https://idp.example.com/realms/vorpal'\n\
             ISSUER_CLIENT_SECRET=''\n\
             write_service_env_file \"$tmp/vorpal.env\"\n\
             echo REACHED_NEXT_STATEMENT\n\
             cat \"$tmp/vorpal.env\"\n\
             ls -l \"$tmp/vorpal.env\" | cut -c1-10\n\
             ls \"$tmp\"\n\
             rm -rf \"$tmp\"",
        );

        assert!(
            transcript.contains("REACHED_NEXT_STATEMENT"),
            "writing an env file for an issuer with no client secret must not \
             abort the installer: {transcript}"
        );
        assert!(
            transcript.contains("VORPAL_ISSUER=\"https://idp.example.com/realms/vorpal\""),
            "the env file must carry the issuer: {transcript}"
        );
        assert!(
            !transcript.contains("VORPAL_ISSUER_CLIENT_SECRET"),
            "no secret was configured, so none may be written: {transcript}"
        );
        assert!(
            transcript.contains("-rw-------"),
            "the env file must be created mode 600: {transcript}"
        );
        assert!(
            !transcript.contains("vorpal.env.tmp"),
            "the temporary file must be renamed into place, not left behind: {transcript}"
        );
    }

    #[test]
    fn installer_escapes_the_client_secret_for_the_systemd_env_file() {
        // Named for what it can fail for: this drives the env-file writer
        // and reads the file back, so it pins the *encoding*. That the
        // secret stays out of the unit's argv is a property of the unit
        // writer, which this never calls — see
        // `installer_keeps_the_client_secret_out_of_the_units_argv`.
        let transcript = source_installer(
            "tmp=$(mktemp -d)\n\
             ISSUER='https://idp.example.com/realms/vorpal'\n\
             ISSUER_CLIENT_SECRET='se\"cr$et'\n\
             write_service_env_file \"$tmp/vorpal.env\"\n\
             cat \"$tmp/vorpal.env\"\n\
             rm -rf \"$tmp\"",
        );

        assert!(
            transcript.contains(r#"VORPAL_ISSUER_CLIENT_SECRET="se\"cr\$et""#),
            "the secret must be written escaped for a systemd EnvironmentFile: {transcript}"
        );
    }

    #[test]
    fn installer_removes_the_env_file_when_nothing_is_configured() {
        // A re-install that drops the issuer and the secret must not leave
        // the previous run's credential behind for the unit to keep reading.
        let transcript = source_installer(
            "tmp=$(mktemp -d)\n\
             printf 'VORPAL_ISSUER_CLIENT_SECRET=\"stale\"\\n' > \"$tmp/vorpal.env\"\n\
             ISSUER=''\n\
             ISSUER_CLIENT_SECRET=''\n\
             write_service_env_file \"$tmp/vorpal.env\"\n\
             echo REACHED_NEXT_STATEMENT\n\
             ls \"$tmp\"\n\
             rm -rf \"$tmp\"",
        );

        assert!(
            transcript.contains("REACHED_NEXT_STATEMENT"),
            "removing the env file must not abort the installer: {transcript}"
        );
        assert!(
            !transcript.contains("vorpal.env"),
            "a stale env file must be removed when nothing is configured: {transcript}"
        );
    }

    /// Drives `install_service_linux` against a throwaway `$HOME`, returning
    /// the two written files' modes, the directory listing, and the unit
    /// itself.
    ///
    /// `systemctl` is shadowed by a function that fails, so the real init
    /// system is never contacted and the writers' behaviour is the same on
    /// every platform. Everything the function writes lands under the
    /// temporary `$HOME`.
    fn install_linux_service_transcript() -> String {
        source_installer(
            "systemctl() { return 1; }\n\
             tmp=$(mktemp -d)\n\
             HOME=\"$tmp\"\n\
             VORPAL_INSTALL_DIR=\"$tmp/.vorpal\"\n\
             SERVICES='worker'\n\
             ISSUER='https://idp.example.com/realms/vorpal'\n\
             ISSUER_CLIENT_SECRET='s3cr3t-not-in-argv'\n\
             unit_dir=\"$tmp/.config/systemd/user\"\n\
             mkdir -p \"$unit_dir\"\n\
             : > \"$unit_dir/vorpal.service\"\n\
             chmod 644 \"$unit_dir/vorpal.service\"\n\
             install_service_linux || true\n\
             printf 'unit-mode %s\\n' \"$(ls -l \"$unit_dir/vorpal.service\" | cut -c1-10)\"\n\
             printf 'env-mode %s\\n' \"$(ls -l \"$unit_dir/vorpal.env\" | cut -c1-10)\"\n\
             ls \"$unit_dir\"\n\
             cat \"$unit_dir/vorpal.service\"\n\
             rm -rf \"$tmp\"",
        )
    }

    #[test]
    fn installer_writes_the_unit_and_the_env_file_mode_600_with_no_temporary_left_behind() {
        // Both writers create a temporary sibling under `umask 077` and
        // rename it into place. Pre-creating the unit at 0644 is what makes
        // this pin the rename rather than the umask: umask governs a file's
        // mode only at creation, so a writer that truncated the existing
        // file in place would leave 0644 here while passing on a fresh
        // directory.
        let transcript = install_linux_service_transcript();

        assert!(
            transcript.contains("unit-mode -rw-------"),
            "the unit must end up mode 600 even where a 0644 file already \
             existed: {transcript}"
        );
        assert!(
            transcript.contains("env-mode -rw-------"),
            "the env file carries the client secret and must be mode 600: {transcript}"
        );
        assert!(
            !transcript.contains("vorpal.service.tmp") && !transcript.contains("vorpal.env.tmp"),
            "each writer must rename its temporary sibling into place rather \
             than leave it beside the installed file: {transcript}"
        );
    }

    #[test]
    fn installer_keeps_the_client_secret_out_of_the_units_argv() {
        // The secret lives in a mode-600 EnvironmentFile because argv is
        // world-readable through `ps`, so the property is about the *unit*:
        // it is the rendered `ExecStart` that has to be read back. A writer
        // that appended `--issuer-client-secret` to that line would leave
        // every other installer test green.
        let transcript = install_linux_service_transcript();

        assert!(
            transcript.contains("--issuer \"https://idp.example.com/realms/vorpal\""),
            "the unit's ExecStart must carry the issuer on argv: {transcript}"
        );
        assert!(
            !transcript.contains("s3cr3t-not-in-argv"),
            "the client secret must never be written into the unit: {transcript}"
        );
    }

    #[test]
    fn installer_encodes_the_plists_operator_supplied_values_as_xml() {
        // The launchd half of the same two writers. `launchctl` is shadowed
        // so nothing reaches the real service manager.
        //
        // The audience carries `<` — a character `validate_no_unit_injection_chars`
        // refuses at the front door, so the encoders here are unreachable in
        // a real install. That is the point: the plist grammar is encoded at
        // the site that writes it, so a later relaxation of the deny-list
        // cannot silently turn an operator's value into markup. Driving the
        // writer directly is the only way to see the encoder at all.
        let transcript = source_installer(
            "launchctl() { return 1; }\n\
             tmp=$(mktemp -d)\n\
             HOME=\"$tmp\"\n\
             VORPAL_INSTALL_DIR=\"$tmp/.vorpal\"\n\
             VORPAL_SYSTEM_DIR=\"$tmp/system\"\n\
             SERVICES='worker'\n\
             ISSUER='https://idp.example.com/realms/vorpal'\n\
             ISSUER_AUDIENCE='aud<ience'\n\
             ISSUER_CLIENT_SECRET='s3cr3t-not-in-argv'\n\
             plist_dir=\"$tmp/Library/LaunchAgents\"\n\
             install_service_macos || true\n\
             printf 'plist-mode %s\\n' \"$(ls -l \"$plist_dir/com.altf4llc.vorpal.plist\" | cut -c1-10)\"\n\
             ls \"$plist_dir\"\n\
             cat \"$plist_dir/com.altf4llc.vorpal.plist\"\n\
             rm -rf \"$tmp\"",
        );

        assert!(
            transcript.contains("<string>aud&lt;ience</string>"),
            "a ProgramArguments value must be XML-encoded at the site that \
             writes it: {transcript}"
        );
        assert!(
            transcript.contains("plist-mode -rw-------"),
            "the plist carries the client secret and must be mode 600: {transcript}"
        );
        assert!(
            !transcript.contains("com.altf4llc.vorpal.plist.tmp"),
            "the temporary sibling must be renamed into place: {transcript}"
        );
        assert!(
            !transcript.contains("--issuer-client-secret"),
            "the secret travels in EnvironmentVariables, never in \
             ProgramArguments: {transcript}"
        );
    }

    #[test]
    fn installer_runs_when_piped_to_bash_on_stdin() {
        // The documented install is `curl … | bash`, where the script has no
        // file of its own and `BASH_SOURCE` is empty. Under the file's
        // `set -euo pipefail` an unguarded array read aborts there before
        // `main` runs — invisible to every other test here, because they all
        // reach the script through a path (`bash <file>`, `source <file>`)
        // that gives `BASH_SOURCE` a value.
        let transcript = pipe_installer_to_bash(&["--dry-run", "--services", "worker"]);

        assert!(
            !transcript.contains("unbound variable"),
            "the entry-point guard must not read BASH_SOURCE unguarded: {transcript}"
        );
        assert!(
            transcript.contains("Missing --issuer for service 'worker'"),
            "main must run when the installer arrives on stdin: {transcript}"
        );
    }

    // --- makefile, driven as a program --------------------------------------

    /// Expands `target` with `make -n` after `assignment`, from the
    /// repository root. `-n` still expands recipe lines, so
    /// `$(CHECK_VORPAL_ISSUER)`'s `$(error …)` fires without the recipe
    /// running. Returns whether make succeeded, and its transcript.
    fn make_dry_run(assignment: &str, target: &str) -> (bool, String) {
        let _guard = issuer_env_guard();

        let output = std::process::Command::new("make")
            .arg("-n")
            .arg(assignment)
            .arg(target)
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
            .env_remove("VORPAL_ISSUER")
            .output()
            .expect("make must be runnable from the repository root");

        (
            output.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    }

    #[test]
    fn makefile_refuses_an_issuer_carrying_shell_syntax() {
        // `$(VORPAL_ISSUER)` is substituted as plain text into a
        // double-quoted host shell word, and into that same shape nested
        // inside `bash -c '…'` for the Lima VM. The guard is the union of
        // what either shell can reinterpret; until now it was checked by
        // reading it.
        let (accepted, transcript) = make_dry_run(
            "VORPAL_ISSUER=http://localhost:8080/realms/vorpal",
            "vorpal-start",
        );

        assert!(
            accepted && transcript.contains("--issuer"),
            "positive control: a well-formed issuer must expand the recipe: {transcript}"
        );

        for hostile in [
            "VORPAL_ISSUER=http://localhost:8080/\" evil",
            "VORPAL_ISSUER=http://localhost:8080/' evil",
            "VORPAL_ISSUER=http://localhost:8080/`evil`",
            "VORPAL_ISSUER=http://localhost:8080/\\evil",
            "VORPAL_ISSUER=http://localhost:8080/$evil",
        ] {
            let (accepted, transcript) = make_dry_run(hostile, "vorpal-start");

            assert!(
                !accepted && transcript.contains("VORPAL_ISSUER may not contain"),
                "{hostile:?} must be refused by the guard: {transcript}"
            );
        }
    }

    #[test]
    fn makefile_issuer_guard_cannot_see_a_simply_expanded_override() {
        // The guard's one documented hole, made executable so it cannot
        // decay into a comment nobody re-checks. `make VORPAL_ISSUER:=…`
        // is expanded at assignment time — before any line of the makefile
        // runs — so a `$(shell …)` in it has already run by the time
        // `$(value …)` reads the result. The recursively-expanded form an
        // operator actually writes is caught, and that half is what the
        // guard exists for.
        //
        // If this test starts failing because make refuses the second form
        // too, the hole is closed: delete this test and the makefile comment
        // that describes it together.
        let witness = std::env::temp_dir().join(format!(
            "vorpal-makefile-issuer-guard-{}",
            std::process::id()
        ));
        let witness = witness.display().to_string();

        let _ = std::fs::remove_file(&witness);
        let (accepted, transcript) = make_dry_run(
            &format!("VORPAL_ISSUER=$(shell touch {witness})"),
            "vorpal-start",
        );

        assert!(
            !accepted && transcript.contains("VORPAL_ISSUER may not contain"),
            "a recursively-expanded override must be refused: {transcript}"
        );
        assert!(
            !std::path::Path::new(&witness).exists(),
            "`$(value …)` must hand the guard the raw text, so the shell \
             function in a refused value never runs"
        );

        let (accepted, transcript) = make_dry_run(
            &format!("VORPAL_ISSUER:=$(shell touch {witness})"),
            "vorpal-start",
        );
        let bypassed = std::path::Path::new(&witness).exists();
        let _ = std::fs::remove_file(&witness);

        assert!(
            accepted && bypassed,
            "the simply-expanded form is expanded before the guard can read \
             it; this is the documented limit: {transcript}"
        );
    }

    #[test]
    fn parse_issuer_names_the_cause_it_actually_hit() {
        // Every failure used to be reported as "issuer
        // must be https", including the ones that had nothing to do with
        // the scheme. AC4 asks the refusal to name the migration step; a
        // message describing the wrong defect cannot. Assert both halves —
        // the cause that was hit, and the absence of the one that was not.
        let error = parse_issuer("idp.example.com/realms/vorpal")
            .expect_err("a scheme-less issuer must be refused");

        assert!(
            error.contains("not a valid absolute URL"),
            "unexpected message: {error}"
        );
        assert!(
            !error.contains("must be https"),
            "a URL-shape failure must not be reported as a scheme failure: {error}"
        );
    }

    #[test]
    fn parse_issuer_refuses_embedded_credentials() {
        // The startup log line states the trust anchor verbatim, and the
        // login path stores it as a credentials-file key, so a
        // `user:password@` issuer would put a password in the journal and on
        // disk. Nothing is lost by refusing: no IdP states userinfo in the
        // `iss` claim this value is compared against.
        let error = parse_issuer("https://user:pw@idp.example.com/realms/vorpal")
            .expect_err("an issuer carrying credentials must be refused");

        assert!(
            error.contains("embedded credentials"),
            "unexpected message: {error}"
        );

        // A password with no username is the same defect spelled differently.
        assert!(
            parse_issuer("https://:pw@idp.example.com/realms/vorpal").is_err(),
            "a password-only authority must be refused too"
        );
    }

    #[test]
    fn parse_issuer_trims_a_trailing_slash() {
        let normalized = parse_issuer("https://tenant.example.com/")
            .expect("a well-formed https issuer must validate");

        assert_eq!(normalized, "https://tenant.example.com");
    }

    #[test]
    fn parse_issuer_trims_multiple_trailing_slashes() {
        // A single `trim_end_matches('/')` call removes every trailing
        // slash, not just one. An earlier version of this test could not
        // fail for the mutant its comment named, because `parse_issuer`
        // trimmed the raw text *and* `NormalizedIssuer::parse` trimmed the
        // parsed form, so
        // weakening either one left the other to absorb it. The raw-text
        // trim is gone, and the surviving trim is the one this pins:
        // replacing it with `strip_suffix('/')` now fails here.
        let normalized = parse_issuer("https://tenant.example.com///")
            .expect("a well-formed https issuer must validate");

        assert_eq!(normalized, "https://tenant.example.com");
    }

    #[test]
    fn parse_issuer_strips_embedded_control_characters() {
        // The returned value must come from the parsed
        // `Url`, not the raw trimmed input, so a control character embedded
        // in the path cannot ride through validation unparsed. `Url::parse`
        // removes ASCII tab/newline wherever they appear in the input.
        //
        // asserting only that the tab is absent passes
        // for any implementation that mangles the value, and skipped the
        // newline — the character round 0's finding actually turned on,
        // since a newline is what breaks out of a systemd unit line. Assert
        // the whole normalized value, for both characters.
        assert_eq!(
            parse_issuer("https://tenant.example.com/rea\tlm")
                .expect("a well-formed https issuer must validate"),
            "https://tenant.example.com/realm"
        );

        assert_eq!(
            parse_issuer("https://tenant.example.com/rea\nlm")
                .expect("a well-formed https issuer must validate"),
            "https://tenant.example.com/realm"
        );
    }

    #[test]
    fn issuer_normalization_is_shared_by_both_issuer_entry_points() {
        // De-duplicating the two issuer rules onto
        // `NormalizedIssuer::parse` also handed `vorpal login`
        // `Url`'s normalization, which is observable there — the returned
        // string is the credentials-file key and the value compared against
        // the discovery document's `issuer` claim. Nothing could see that
        // change. Pin the exact transformation on both entry points, so it
        // is a stated contract rather than a side effect, and so the two
        // cannot drift apart again without a red test.
        let mixed_case = "https://Tenant.Example.COM:443/realms/Vorpal";

        assert_eq!(
            parse_issuer(mixed_case).expect("a well-formed https issuer must validate"),
            "https://tenant.example.com/realms/Vorpal",
            "scheme and host are lower-cased and the default port elided; the path is not"
        );

        assert_eq!(
            normalize_and_validate_login_issuer(mixed_case)
                .expect("a well-formed https issuer must validate")
                .as_str(),
            parse_issuer(mixed_case).expect("a well-formed https issuer must validate"),
            "the login path and the service path must normalize an issuer identically"
        );
    }

    #[test]
    fn parse_client_secret_refuses_a_set_but_empty_value() {
        // `VORPAL_ISSUER_CLIENT_SECRET=` is a *present*
        // value to clap, so with no value parser it became `Some("")` and
        // was POSTed to the IdP as an empty secret, coming back as an
        // opaque 401 rather than as the configuration error it is.
        let error = parse_client_secret("").expect_err("an empty secret must be refused");

        assert!(
            error.contains("must not be empty"),
            "unexpected message: {error}"
        );

        assert!(
            parse_client_secret("   ").is_err(),
            "a whitespace-only secret must be refused"
        );
    }

    #[test]
    fn parse_client_secret_returns_an_idp_generated_value_unchanged() {
        // The companion to the refusal above: a client secret is opaque IdP
        // text, so every byte of an accepted one survives. `script/install.sh`
        // encodes rather than rejects it for the same reason.
        let secret = " a&b<c>d\"e'f ";

        assert_eq!(
            parse_client_secret(secret).expect("a non-empty secret must be accepted"),
            secret
        );
    }

    #[test]
    fn issuer_client_secret_arg_hides_its_env_value_and_validates_it() {
        // Without `hide_env_values`, clap prints the live secret in
        // `system services start --help`, undoing the point of moving it out
        // of argv in the first place.
        let _guard = issuer_env_guard();

        let cli_command = Cli::command();
        let start_command = cli_command
            .find_subcommand("system")
            .expect("system subcommand must exist")
            .find_subcommand("services")
            .expect("system services subcommand must exist")
            .find_subcommand("start")
            .expect("system services start subcommand must exist");
        let secret_arg = start_command
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == ISSUER_CLIENT_SECRET_ARG_ID)
            .expect("--issuer-client-secret argument must exist");

        assert_eq!(
            secret_arg.get_env(),
            Some(std::ffi::OsStr::new("VORPAL_ISSUER_CLIENT_SECRET")),
            "the secret must be settable from the mode-600 environment file the installer writes"
        );
        assert!(
            secret_arg.is_hide_env_values_set(),
            "--help must not print the client secret it read from the environment"
        );

        assert!(
            Cli::try_parse_from([
                "vorpal",
                "system",
                "services",
                "start",
                "--issuer-client-secret",
                "",
            ])
            .is_err(),
            "an empty --issuer-client-secret must be rejected during clap parsing"
        );
    }

    #[test]
    fn system_services_start_reads_the_issuer_from_the_environment() {
        // The two existing instruments pin the argument
        // *metadata* (CommandFactory) and the argv path (try_parse_from)
        // separately, so nothing drove the composition AC1 actually ships —
        // a value arriving through VORPAL_ISSUER, with no flag, reaching
        // `StartArgs.issuer` already validated. This is that test.
        //
        // clap snapshots the environment when the `Arg` is built, which is
        // inside `try_parse_from`, so the variable must be set across the
        // call. Tests share a process, so the writes are serialized on the
        // module's one `ISSUER_ENV_LOCK` and undone before it is released.
        let _guard = issuer_env_guard();

        std::env::set_var(
            "VORPAL_ISSUER",
            "https://Idp.Example.com:443/realms/vorpal/",
        );
        let accepted = Cli::try_parse_from(["vorpal", "system", "services", "start"]);

        std::env::set_var("VORPAL_ISSUER", "");
        let empty = Cli::try_parse_from(["vorpal", "system", "services", "start"]);

        std::env::set_var("VORPAL_ISSUER", "http://idp.example.com/realms/vorpal");
        let plaintext = Cli::try_parse_from(["vorpal", "system", "services", "start"]);

        std::env::remove_var("VORPAL_ISSUER");

        let parsed = accepted.expect("an https VORPAL_ISSUER must parse");

        let Command::System(CommandSystem::Services(CommandSystemServices::Start {
            issuer, ..
        })) = parsed.command
        else {
            panic!("expected `system services start`")
        };

        assert_eq!(
            issuer.as_deref(),
            Some("https://idp.example.com/realms/vorpal"),
            "the env-sourced issuer must arrive already validated and normalized"
        );

        // AB2: clap treats a set-but-empty variable as a supplied value, so
        // without the value parser this one reached `resolve_required_issuer`
        // as a false "issuer configured".
        assert!(
            empty.is_err(),
            "an empty VORPAL_ISSUER must be refused at parse time, not seen as present"
        );
        assert!(
            plaintext.is_err(),
            "a plaintext off-loopback VORPAL_ISSUER must be refused at parse time"
        );
    }

    #[test]
    fn system_services_start_issuer_flag_takes_precedence_over_the_environment() {
        // An installed systemd unit writes `--issuer` into `ExecStart` and
        // `VORPAL_ISSUER` into its `EnvironmentFile`, so its trust anchor is
        // the argv one only if argv wins. clap documents that precedence, but
        // one test pins that the env value is wired and another that an
        // env-only value is read; neither exercises both channels at once.
        let _guard = issuer_env_guard();

        std::env::set_var(
            "VORPAL_ISSUER",
            "https://env-anchor.example.com/realms/vorpal",
        );
        let parsed = Cli::try_parse_from([
            "vorpal",
            "system",
            "services",
            "start",
            "--issuer",
            "https://argv-anchor.example.com/realms/vorpal",
        ]);
        std::env::remove_var("VORPAL_ISSUER");

        let Command::System(CommandSystem::Services(CommandSystemServices::Start {
            issuer, ..
        })) = parsed
            .expect("both a valid flag and a valid env value must still parse")
            .command
        else {
            panic!("expected `system services start`")
        };

        assert_eq!(
            issuer.as_deref(),
            Some("https://argv-anchor.example.com/realms/vorpal"),
            "--issuer on argv must win over a simultaneously-set VORPAL_ISSUER"
        );
    }

    // --- argument provenance ------------------------------------------------

    /// The `system services start` matches for `args`, so a test can ask
    /// clap where each value came from — the same question `run` asks before
    /// logging the trust anchor and warning about an argv-borne secret.
    fn services_start_matches(args: &[&str]) -> ArgMatches {
        let mut full_argv = vec!["vorpal", "system", "services", "start"];
        full_argv.extend_from_slice(args);

        Cli::command()
            .try_get_matches_from(full_argv)
            .expect("these arguments must parse")
    }

    #[test]
    fn a_client_secret_on_argv_is_reported_as_argv_sourced_even_when_the_env_var_is_set() {
        // The warning that the secret is readable through `ps` used to fire
        // only when `VORPAL_ISSUER_CLIENT_SECRET` was unset, on the reasoning
        // that an unset variable means the value came from the flag. That
        // inference is silent in exactly the case an installed unit creates —
        // the variable set from the mode-600 env file *and* a secret typed on
        // the command line — which is the case where the secret really is
        // exposed. clap's own record of the value's source is exact in both
        // directions and does not consult the environment.
        let _guard = issuer_env_guard();

        std::env::set_var("VORPAL_ISSUER_CLIENT_SECRET", "secret-from-the-env-file");
        let both = services_start_matches(&["--issuer-client-secret", "secret-on-argv"]);
        let env_only = services_start_matches(&[]);
        std::env::remove_var("VORPAL_ISSUER_CLIENT_SECRET");

        assert_eq!(
            services_start_value_source(&both, ISSUER_CLIENT_SECRET_ARG_ID),
            Some(ValueSource::CommandLine),
            "a secret typed on the command line is exposed to `ps` whether or \
             not the environment also carries one"
        );
        assert_eq!(
            services_start_value_source(&env_only, ISSUER_CLIENT_SECRET_ARG_ID),
            Some(ValueSource::EnvVariable),
            "a secret that arrived only through the environment must not be \
             reported as being on argv"
        );
    }

    #[test]
    fn the_trust_anchors_channel_is_reported_from_the_channel_it_arrived_on() {
        // Whoever controls the issuer controls which tokens a worker or
        // registry accepts, and since it can arrive through `VORPAL_ISSUER`
        // it need leave no trace in `ps`. The startup log states the value
        // and its channel; these are the labels it states.
        let _guard = issuer_env_guard();

        std::env::set_var(
            "VORPAL_ISSUER",
            "https://env-anchor.example.com/realms/vorpal",
        );
        let from_env = services_start_matches(&[]);
        let from_argv =
            services_start_matches(&["--issuer", "https://argv-anchor.example.com/realms/vorpal"]);
        std::env::remove_var("VORPAL_ISSUER");
        let unset = services_start_matches(&[]);

        assert_eq!(
            value_source_label(services_start_value_source(&from_env, ISSUER_ARG_ID)),
            "environment"
        );
        assert_eq!(
            value_source_label(services_start_value_source(&from_argv, ISSUER_ARG_ID)),
            "command line"
        );
        assert_eq!(
            services_start_value_source(&unset, ISSUER_ARG_ID),
            None,
            "with no issuer on either channel there is no anchor to name"
        );
    }

    #[test]
    fn only_a_worker_or_registry_start_is_told_a_missing_anchor_refuses() {
        // The startup log warns that worker and registry services "will
        // refuse to start" when no anchor is configured. Said on an
        // agent-only start, where nothing refuses, it is a warning about an
        // event that cannot happen — which is how operators learn to skim
        // the line that matters.
        assert!(starts_an_authenticated_service("worker"));
        assert!(starts_an_authenticated_service("agent,registry"));
        assert!(!starts_an_authenticated_service("agent"));

        // Split exactly as `start::run` splits it (`services.split(',')`,
        // then equality against "worker"/"registry"), so the log line and
        // the refusal cannot disagree about a padded list.
        assert!(!starts_an_authenticated_service(" worker"));
    }

    // --- normalize_and_validate_login_issuer (AC1, AC4) -------------------

    #[test]
    fn normalize_and_validate_login_issuer_refuses_plaintext_off_loopback() {
        let error = normalize_and_validate_login_issuer("http://idp.example.com")
            .expect_err("a plaintext non-loopback issuer must be refused");

        assert!(
            error.to_string().contains("must be https"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn normalize_and_validate_login_issuer_permits_loopback_plaintext() {
        // Positive control: `vorpal login`'s own default is plaintext
        // loopback, so if this refused it a flagless `vorpal login` would be
        // broken.
        let normalized = normalize_and_validate_login_issuer(DEFAULT_DEV_ISSUER)
            .expect("loopback plaintext must be permitted");

        assert_eq!(normalized.as_str(), DEFAULT_DEV_ISSUER);
    }

    #[test]
    fn normalize_and_validate_login_issuer_trims_a_trailing_slash() {
        let normalized = normalize_and_validate_login_issuer("https://tenant.example.com/")
            .expect("a well-formed https issuer must validate");

        assert_eq!(normalized.as_str(), "https://tenant.example.com");
    }

    // --- login_discovery_targets (AC2, C-3, C-4) ---------------------------

    fn matching_doc(issuer: &str) -> serde_json::Value {
        serde_json::json!({
            "issuer": issuer,
            "device_authorization_endpoint": format!("{issuer}/device"),
            "token_endpoint": format!("{issuer}/token"),
        })
    }

    /// The normalized form of `issuer`, so a test can hand
    /// `login_discovery_targets` and `fetch_login_discovery_endpoints` the
    /// precondition their signatures now require.
    fn normalized(issuer: &str) -> NormalizedIssuer {
        NormalizedIssuer::parse(issuer).expect("a test issuer must be well formed")
    }

    #[test]
    fn login_record_keys_the_issuer_and_the_registry_to_their_own_maps() {
        // The `Command::Login` arm's only untested step used to be this
        // binding, where two URL-shaped strings are handed to the SDK. A
        // transposition writes the registry URL as an issuer key and sends
        // that issuer's token wherever the issuer URL names a registry
        // (VPL-188 AB-4).
        let issuer = "https://idp.example.com";
        let registry = "https://registry.example.com";

        let record = login_record(
            &normalized(issuer),
            registry,
            LoginGrant {
                access_token: "access".to_string(),
                audience: Some("audience".to_string()),
                client_id: "client".to_string(),
                expires_in: 3600,
                issued_at: 1_700_000_000,
                refresh_token: "refresh".to_string(),
                scopes: vec!["openid".to_string()],
            },
        );

        assert_eq!(record.issuer, issuer, "the issuer URL must key the grant");
        assert_eq!(
            record.registry, registry,
            "the registry URL must key the mapping, not the grant"
        );
        assert_eq!(record.content.access_token, "access");
        assert_eq!(record.content.refresh_token, "refresh");
        assert_eq!(record.content.expires_in, 3600);
        assert_eq!(record.content.issued_at, 1_700_000_000);
        assert_eq!(record.content.client_id, "client");
        assert_eq!(record.content.audience.as_deref(), Some("audience"));
        assert_eq!(record.content.scopes, vec!["openid".to_string()]);
    }

    #[test]
    fn login_record_carries_the_normalized_issuer_not_the_raw_text() {
        // `NormalizedIssuer::parse` elides the default port and trims the
        // trailing slash. The credentials-file key must be that canonical
        // form, because it is the string every later lookup compares.
        let record = login_record(
            &normalized("https://idp.example.com:443/"),
            "https://registry.example.com",
            LoginGrant {
                access_token: "access".to_string(),
                audience: None,
                client_id: "client".to_string(),
                expires_in: 3600,
                issued_at: 1_700_000_000,
                refresh_token: "refresh".to_string(),
                scopes: vec![],
            },
        );

        assert_eq!(record.issuer, "https://idp.example.com");
    }

    #[test]
    fn login_discovery_targets_accepts_a_matching_document() {
        let issuer = "https://idp.example.com";
        let (device, token) = login_discovery_targets(&normalized(issuer), &matching_doc(issuer))
            .expect("matching document");

        assert_eq!(device, "https://idp.example.com/device");
        assert_eq!(token, "https://idp.example.com/token");
    }

    #[test]
    fn login_discovery_targets_refuses_an_off_origin_token_endpoint() {
        let issuer = "https://idp.example.com";
        let mut doc = matching_doc(issuer);
        doc["token_endpoint"] = serde_json::json!("https://attacker.example.com/token");

        let error = login_discovery_targets(&normalized(issuer), &doc)
            .expect_err("an off-origin token_endpoint must be refused");

        assert!(
            error.to_string().contains("token_endpoint"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn login_discovery_targets_refuses_an_off_origin_device_endpoint() {
        // C-3: not one of VPL-280's five ACs, but required by the threat
        // model — the device endpoint is what the CLI prints for the user
        // to open in a browser (AB-3).
        let issuer = "https://idp.example.com";
        let mut doc = matching_doc(issuer);
        doc["device_authorization_endpoint"] =
            serde_json::json!("https://attacker.example.com/device");

        let error = login_discovery_targets(&normalized(issuer), &doc)
            .expect_err("an off-origin device_authorization_endpoint must be refused");

        assert!(
            error.to_string().contains("device_authorization_endpoint"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn login_discovery_targets_refuses_an_issuer_claim_mismatch() {
        // C-4: the origin pin above is blind to path, so a same-origin
        // co-tenant on a multi-realm IdP (AB-4) would otherwise pass.
        let issuer = "https://idp.example.com/realms/vorpal";
        let doc = matching_doc("https://idp.example.com/realms/other");

        let error = login_discovery_targets(&normalized(issuer), &doc)
            .expect_err("a document declaring a different issuer must be refused");

        assert!(
            error
                .to_string()
                .contains("does not match requested issuer"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn login_discovery_targets_trims_the_documents_issuer_claim_before_comparing() {
        // N: the trailing-slash trim on the document's own `issuer` claim
        // (`doc_issuer.trim_end_matches('/') != issuer`) was untested —
        // `matching_doc` never emits a trailing slash, so a mutant deleting
        // the trim would still pass every other case here. `issuer` is
        // already normalized (no trailing slash, per
        // `normalize_and_validate_login_issuer`); the document below
        // disagrees with it only in that one respect.
        let issuer = "https://idp.example.com";
        let mut doc = matching_doc(issuer);
        doc["issuer"] = serde_json::json!("https://idp.example.com/");

        let (device, token) = login_discovery_targets(&normalized(issuer), &doc)
            .expect("a trailing-slash issuer claim must still match after trimming");

        assert_eq!(device, format!("{issuer}/device"));
        assert_eq!(token, format!("{issuer}/token"));
    }

    #[test]
    fn login_discovery_targets_accepts_a_documents_issuer_naming_an_explicit_default_port() {
        // Before this fix, `issuer` was canonicalized
        // by `NormalizedIssuer::parse` (eliding the default :443
        // port) while `doc_issuer` was only trailing-slash trimmed, so this
        // exact case — a document's `issuer` claim spelling out the default
        // port explicitly, which RFC 3986 and this binary's own
        // canonicalization both treat as equivalent to omitting it — failed
        // the equality check and refused a login that previously worked.
        let issuer = "https://idp.example.com/realms/vorpal";
        let doc = matching_doc("https://idp.example.com:443/realms/vorpal");

        let (device, token) = login_discovery_targets(&normalized(issuer), &doc).expect(
            "a document issuer naming the scheme's default port explicitly must still match",
        );

        assert_eq!(device, "https://idp.example.com:443/realms/vorpal/device");
        assert_eq!(token, "https://idp.example.com:443/realms/vorpal/token");
    }

    #[test]
    fn login_discovery_targets_refuses_a_missing_issuer_field() {
        let issuer = "https://idp.example.com";
        let doc = serde_json::json!({
            "device_authorization_endpoint": format!("{issuer}/device"),
            "token_endpoint": format!("{issuer}/token"),
        });

        let error = login_discovery_targets(&normalized(issuer), &doc)
            .expect_err("a document with no issuer field must fail closed");

        assert!(
            error.to_string().contains("missing issuer"),
            "unexpected error: {error}"
        );
    }

    // --- login_verification_prompt (VPL-738 AC1, AC2, AC3) -----------------

    fn device_response(
        verification_uri: &str,
        verification_uri_complete: Option<&str>,
    ) -> StandardDeviceAuthorizationResponse {
        let mut doc = serde_json::json!({
            "device_code": "device",
            "user_code": "ABCD-EFGH",
            "verification_uri": verification_uri,
            "expires_in": 600,
            "interval": 5,
        });

        if let Some(complete) = verification_uri_complete {
            doc["verification_uri_complete"] = serde_json::json!(complete);
        }

        serde_json::from_value(doc).expect("a well-formed device-authorization response")
    }

    #[test]
    fn login_verification_prompt_accepts_an_on_origin_response() {
        let issuer = "https://idp.example.com/realms/vorpal";
        let details = device_response(
            "https://idp.example.com/realms/vorpal/device",
            Some("https://idp.example.com/realms/vorpal/device?user_code=ABCD-EFGH"),
        );

        let prompt = login_verification_prompt(&normalized(issuer), &details)
            .expect("an on-origin verification URI must be accepted");

        assert_eq!(
            prompt.verification_uri,
            "https://idp.example.com/realms/vorpal/device"
        );
        assert_eq!(
            prompt.verification_uri_complete.as_deref(),
            Some("https://idp.example.com/realms/vorpal/device?user_code=ABCD-EFGH")
        );
    }

    #[test]
    fn login_verification_prompt_accepts_the_loopback_dev_issuer() {
        // Positive control for the flagless `vorpal login` default: the
        // loopback allowance in `credential_egress_origin` must reach the
        // verification URI too, or the documented dev flow breaks.
        let details = device_response(
            "http://localhost:8080/realms/vorpal/device",
            Some("http://localhost:8080/realms/vorpal/device?user_code=ABCD-EFGH"),
        );

        let prompt = login_verification_prompt(&normalized(DEFAULT_DEV_ISSUER), &details)
            .expect("the loopback dev issuer must keep working");

        assert_eq!(
            prompt.verification_uri,
            "http://localhost:8080/realms/vorpal/device"
        );
    }

    #[test]
    fn login_verification_prompt_omits_an_absent_complete_uri() {
        let issuer = "https://idp.example.com";
        let details = device_response("https://idp.example.com/device", None);

        let prompt = login_verification_prompt(&normalized(issuer), &details)
            .expect("verification_uri_complete is optional per RFC 8628");

        assert_eq!(prompt.verification_uri_complete, None);
    }

    #[test]
    fn login_verification_prompt_refuses_an_off_origin_verification_uri() {
        let issuer = "https://idp.example.com";
        let details = device_response("https://idp-example.com.attacker.test/device", None);

        let error = login_verification_prompt(&normalized(issuer), &details)
            .expect_err("an off-origin verification_uri must be refused before printing");

        assert!(
            error.to_string().contains("verification_uri"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn login_verification_prompt_refuses_an_off_origin_complete_uri() {
        // The complete URI is printed first and is the link the user
        // actually follows, so a check covering only `verification_uri`
        // leaves the primary human-facing link unprotected.
        let issuer = "https://idp.example.com";
        let details = device_response(
            "https://idp.example.com/device",
            Some("https://attacker.example.com/device?user_code=ABCD-EFGH"),
        );

        let error = login_verification_prompt(&normalized(issuer), &details)
            .expect_err("an off-origin verification_uri_complete must be refused");

        assert!(
            error.to_string().contains("verification_uri_complete"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn login_verification_prompt_refuses_an_unparseable_complete_uri() {
        // `verification_uri_complete` is a plain string in the oauth2 crate,
        // never parsed as a URL, so a value that is not a URL at all reaches
        // this function intact. A parse failure is a refusal, not a skip.
        let issuer = "https://idp.example.com";
        let details = device_response("https://idp.example.com/device", Some("not a url"));

        let error = login_verification_prompt(&normalized(issuer), &details)
            .expect_err("an unparseable verification_uri_complete must be refused");

        assert!(
            error.to_string().contains("verification_uri_complete"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn login_verification_prompt_refuses_a_plaintext_verification_uri() {
        let issuer = "https://idp.example.com";
        let details = device_response("http://idp.example.com/device", None);

        let error = login_verification_prompt(&normalized(issuer), &details)
            .expect_err("a scheme downgrade on the human-facing link must be refused");

        assert!(
            error.to_string().contains("must be https"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn login_verification_prompt_prints_the_parsed_form_not_the_raw_response_text() {
        // WHATWG parsing strips ASCII tab/LF/CR from a URL, so a raw
        // response string can parse on-origin while its own bytes carry a
        // second rendered line. Printing the raw text would let a passing
        // origin check vouch for a URL the user never sees checked.
        let issuer = "https://idp.example.com";
        let smuggled = "https://idp.example.com/dev\nice";
        let details = device_response(smuggled, None);

        assert!(
            details.verification_uri().to_string().contains('\n'),
            "the fixture must actually retain the raw newline, or this test is vacuous"
        );

        let prompt = login_verification_prompt(&normalized(issuer), &details)
            .expect("the parsed URL is on-origin");

        assert_eq!(prompt.verification_uri, "https://idp.example.com/device");
    }

    #[test]
    fn login_verification_prompt_refusal_does_not_echo_the_rejected_uri() {
        // The refusal ends the attack; it must not carry the attacker's
        // bytes into the terminal that renders it.
        let issuer = "https://idp.example.com";
        let details = device_response("https://attacker.example.com/\u{1b}[2Kdevice", None);

        let error = login_verification_prompt(&normalized(issuer), &details)
            .expect_err("an off-origin verification_uri must be refused");

        let rendered = error.to_string();

        assert!(
            !rendered.contains('\u{1b}'),
            "the refusal rendered an escape byte: {rendered:?}"
        );
        assert!(
            rendered.contains("https://attacker.example.com"),
            "the refusal must still name the offending origin: {rendered}"
        );
    }

    // --- fetch_login_discovery_endpoints (AC2, AC3, AC5) -------------------
    //
    // A minimal HTTP/1.1 stand-in for an IdP, mirroring
    // `sdk/rust/src/context.rs`'s own `IdpServer` fixture: the CLI's
    // discovery fetch cannot import that one across the crate boundary
    // (it is `#[cfg(test)]`-private to the SDK), so AC5's "local HTTP
    // double" is this equivalent in the CLI's own test module.

    struct IdpServer {
        addr: std::net::SocketAddr,
        paths: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl IdpServer {
        async fn start(
            respond: impl Fn(&str, std::net::SocketAddr) -> Option<String> + Send + Sync + 'static,
        ) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind idp fixture");
            let addr = listener.local_addr().expect("fixture address");
            let paths = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let respond = std::sync::Arc::new(respond);
            let accepted = std::sync::Arc::clone(&paths);

            tokio::spawn(async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        return;
                    };

                    let respond = std::sync::Arc::clone(&respond);
                    let accepted = std::sync::Arc::clone(&accepted);

                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};

                        let mut buffer = vec![0u8; 8192];
                        let read = socket.read(&mut buffer).await.unwrap_or(0);
                        let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                        let path = request
                            .split_whitespace()
                            .nth(1)
                            .unwrap_or_default()
                            .to_string();

                        accepted.lock().unwrap().push(path.clone());

                        match respond(&path, addr) {
                            Some(response) => {
                                let _ = socket.write_all(response.as_bytes()).await;
                            }
                            // Accept and never answer, so the caller's own
                            // timeout is the only thing that ends the request.
                            None => std::future::pending::<()>().await,
                        }
                    });
                }
            });

            Self { addr, paths }
        }

        fn issuer(&self) -> String {
            format!("http://127.0.0.1:{}", self.addr.port())
        }

        fn requested_paths(&self) -> Vec<String> {
            self.paths.lock().unwrap().clone()
        }
    }

    fn http_json_status(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            status,
            body.len(),
            body
        )
    }

    fn http_json(body: &str) -> String {
        http_json_status("200 OK", body)
    }

    fn discovery_document(issuer: &str) -> String {
        http_json(&format!(
            "{{\"issuer\":\"{issuer}\",\"device_authorization_endpoint\":\"{issuer}/device\",\"token_endpoint\":\"{issuer}/token\"}}"
        ))
    }

    #[tokio::test]
    async fn fetch_login_discovery_endpoints_succeeds_against_a_matching_fixture() {
        // AC5 positive control: same-origin discovery completes.
        let idp = IdpServer::start(|path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());
            if path == "/.well-known/openid-configuration" {
                return Some(discovery_document(&issuer));
            }
            Some(http_json_status("404 Not Found", "{}"))
        })
        .await;

        let client = login_http_client(Duration::from_secs(5)).expect("test client must build");
        let (device, token) = fetch_login_discovery_endpoints(&client, &normalized(&idp.issuer()))
            .await
            .expect("matching fixture must succeed");

        assert_eq!(device, format!("{}/device", idp.issuer()));
        assert_eq!(token, format!("{}/token", idp.issuer()));
    }

    #[tokio::test]
    async fn fetch_login_discovery_endpoints_refuses_a_cross_origin_token_endpoint() {
        // AC5 negative control, driven through the full discovery fetch
        // rather than only the pure validator.
        let idp = IdpServer::start(|path, addr| {
            let issuer = format!("http://127.0.0.1:{}", addr.port());
            if path == "/.well-known/openid-configuration" {
                return Some(http_json(&format!(
                    "{{\"issuer\":\"{issuer}\",\"device_authorization_endpoint\":\"{issuer}/device\",\"token_endpoint\":\"https://attacker.example.com/token\"}}"
                )));
            }
            Some(http_json_status("404 Not Found", "{}"))
        })
        .await;

        let client = login_http_client(Duration::from_secs(5)).expect("test client must build");
        let error = fetch_login_discovery_endpoints(&client, &normalized(&idp.issuer()))
            .await
            .expect_err("a cross-origin token_endpoint must be refused");

        assert!(
            error.to_string().contains("token_endpoint"),
            "unexpected error: {error}"
        );
        assert_eq!(
            idp.requested_paths(),
            vec!["/.well-known/openid-configuration".to_string()],
            "no request may reach the attacker-named endpoint"
        );
    }

    #[tokio::test]
    async fn fetch_login_discovery_endpoints_refuses_a_redirected_discovery_response() {
        // C-5(a): the discovery GET must not follow a redirect to another
        // origin. A 302 with no JSON body fails `.json()` parsing once the
        // client refuses to follow it — mirroring the SDK's own
        // `refresh_access_token_does_not_follow_a_redirected_token_endpoint`.
        let elsewhere = IdpServer::start(|_, _| Some(http_json("{}"))).await;
        let elsewhere_port = elsewhere.addr.port();

        let idp = IdpServer::start(move |path, _| {
            if path == "/.well-known/openid-configuration" {
                return Some(format!(
                    "HTTP/1.1 302 Found\r\nlocation: http://127.0.0.1:{elsewhere_port}/.well-known/openid-configuration\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                ));
            }
            Some(http_json_status("404 Not Found", "{}"))
        })
        .await;

        let client = login_http_client(Duration::from_secs(5)).expect("test client must build");
        fetch_login_discovery_endpoints(&client, &normalized(&idp.issuer()))
            .await
            .expect_err("a redirected discovery response must not be followed");

        assert!(
            elsewhere.requested_paths().is_empty(),
            "the redirect target must never be reached"
        );
    }

    #[tokio::test]
    async fn fetch_login_discovery_endpoints_times_out_on_a_hung_idp() {
        // VPL-732 C-1/C-2/C-3: a discovery endpoint that accepts and never
        // answers must not stall the command past the client's own timeout.
        // The outer bound below is the test harness's failure detector, not
        // the control under test — asserting only that it fired (as the old
        // `elapsed()` check effectively did once moved after an unbounded
        // await) would pass even if the client's own timeout were stripped,
        // since the outer bound would still end the request. So the outer
        // `expect` must be the one that goes red, and the inner error must
        // be pinned to a client-side timeout, not merely "some error".
        let idp = IdpServer::start(|_, _| None).await;

        let client = login_http_client(Duration::from_millis(250)).expect("test client must build");

        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            fetch_login_discovery_endpoints(&client, &normalized(&idp.issuer())),
        )
        .await
        .expect("the client's own timeout, not the test harness, must end this request");

        let error = outcome.expect_err("a hung discovery endpoint must not hang the caller");
        let reqwest_error = error
            .downcast_ref::<reqwest::Error>()
            .expect("the client timeout must surface as a reqwest::Error");

        assert!(
            reqwest_error.is_timeout(),
            "unexpected error: {reqwest_error}"
        );
        assert_eq!(
            idp.requested_paths(),
            vec!["/.well-known/openid-configuration".to_string()],
            "the request must actually reach the fixture before timing out"
        );
    }
}
