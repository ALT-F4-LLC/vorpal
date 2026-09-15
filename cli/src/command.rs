use crate::command::{
    config::{
        VorpalConfigSource, VorpalConfigSourceGo, VorpalConfigSourcePython, VorpalConfigSourceRust,
        VorpalConfigSourceTypeScript,
    },
    store::paths::get_key_credentials_path,
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
    collections::BTreeMap,
    env::current_dir,
    path::{Path, PathBuf},
    process::exit,
    sync::Mutex,
    time::{Duration, Instant},
};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, time::sleep};
use tracing::{error, subscriber, Level};
use tracing_subscriber::{
    filter::{LevelFilter, Targets},
    layer::{Context, SubscriberExt},
    Layer, Registry,
};
use vorpal_sdk::{
    artifact::{get_default_address, system::get_system_default_str},
    context::{VorpalCredentials, VorpalCredentialsContent, DEFAULT_NAMESPACE},
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

#[derive(Subcommand)]
pub enum CommandSystemKeys {
    Generate {},
}

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

        #[arg(long)]
        issuer: Option<String>,

        #[arg(long)]
        issuer_audience: Option<String>,

        #[arg(long)]
        issuer_client_id: Option<String>,

        #[arg(long)]
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

        /// Enable TLS for the main gRPC listener (requires keys in /var/lib/vorpal/key/)
        #[arg(default_value_t = false, long)]
        tls: bool,
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
        #[arg(long, default_value = "http://localhost:8080/realms/vorpal")]
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
    let discovery_url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );

    let doc: serde_json::Value = reqwest::get(&discovery_url)
        .await?
        .error_for_status()?
        .json()
        .await?;

    let device_endpoint = doc
        .get("device_authorization_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing device_authorization_endpoint"))?;

    let token_endpoint = doc
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("missing token_endpoint"))?;

    let client_device_url = DeviceAuthorizationUrl::new(device_endpoint.to_string())?;

    let client = BasicClient::new(ClientId::new(issuer_client_id.to_string()))
        .set_auth_uri(AuthUrl::new(issuer.to_string())?)
        .set_token_uri(TokenUrl::new(token_endpoint.to_string())?)
        .set_device_authorization_url(client_device_url);

    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build HTTP client")?;

    let mut device_request = client
        .exchange_device_code()
        .add_scope(Scope::new("offline_access".to_string()));

    if let Some(audience) = issuer_audience {
        device_request = device_request.add_extra_param("audience", audience.to_string());
    }

    let details: StandardDeviceAuthorizationResponse =
        device_request.request_async(&http_client).await?;

    if let Some(complete_uri) = details.verification_uri_complete() {
        crate::output::line(format!(
            "Open this URL in your browser:\n{}",
            complete_uri.secret()
        ));
    }

    crate::output::line(format!(
        "Or open {} and enter code: {}",
        details.verification_uri(),
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

    // Prepare to store token

    let content = VorpalCredentialsContent {
        access_token,
        audience: issuer_audience.map(str::to_string),
        client_id: issuer_client_id.to_string(),
        expires_in,
        issued_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("system clock is before the unix epoch")?
            .as_secs(),
        refresh_token,
        scopes,
    };

    // TODO: load existing credentials file if it exists

    let mut issuer_map = BTreeMap::new();
    let mut registry_map = BTreeMap::new();

    issuer_map.insert(issuer.to_string(), content);
    registry_map.insert(registry.to_string(), issuer.to_string());

    let credentials = VorpalCredentials {
        issuer: issuer_map,
        registry: registry_map,
    };
    let credentials_json = serde_json::to_string_pretty(&credentials)?;
    let credentials_path = get_key_credentials_path();

    // Enforce mode 0o600 on file create so the credentials are not
    // born world-readable on a default-umask (022) system. This is
    // the file-birth point — `OpenOptions::mode()` only applies when
    // the file is created, so getting it right here is load-bearing.
    let mut credentials_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&credentials_path)
        .await?;
    credentials_file
        .write_all(credentials_json.as_bytes())
        .await?;
    credentials_file.flush().await?;

    Ok(())
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
async fn dispatch_system(system: CommandSystem) -> Result<()> {
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
                services,
                tls,
            } => {
                // Parse the comma-separated list. Trim whitespace per entry and
                // silently drop empty segments, so inputs like "worker-id,"
                // or " a , , b " never produce `""` entries in the allow-list.
                // Silent-filter matches clap's ergonomic expectation for
                // comma-delimited values and keeps config-by-env forgiving.
                let issuer_service_client_ids = issuer_service_client_ids
                    .as_deref()
                    .map(|raw| {
                        raw.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect::<Vec<String>>()
                    })
                    .unwrap_or_default();

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
                    services: services
                        .split(',')
                        .map(std::string::ToString::to_string)
                        .collect(),
                    tls,
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

        Command::System(system) => dispatch_system(system).await,
    }
}
