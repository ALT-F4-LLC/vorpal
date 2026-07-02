use crate::command::{
    config::{
        VorpalConfigSource, VorpalConfigSourceGo, VorpalConfigSourcePython, VorpalConfigSourceRust,
        VorpalConfigSourceTypeScript,
    },
    store::paths::get_key_credentials_path,
};
use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser, Subcommand};
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
};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, time::sleep};
use tracing::{error, subscriber, Level};
use tracing_subscriber::{
    filter::{LevelFilter, Targets},
    fmt::writer::MakeWriterExt,
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
        /// (default: /var/lib/vorpal/vorpal.sock, override: VORPAL_SOCKET_PATH env var)
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
#[allow(clippy::large_enum_variant)] // top-level CLI subcommand type; single instance per process
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

        /// Artifact agent address (VORPAL_SOCKET_PATH env var overrides default socket path)
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

        /// Registry address (VORPAL_SOCKET_PATH env var overrides default socket path)
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

        /// Artifact worker address (VORPAL_SOCKET_PATH env var overrides default socket path)
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

        /// Artifact agent address (VORPAL_SOCKET_PATH env var overrides default socket path)
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

        /// Registry address (VORPAL_SOCKET_PATH env var overrides default socket path)
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

        /// Artifact worker address (VORPAL_SOCKET_PATH env var overrides default socket path)
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

        /// Registry address (VORPAL_SOCKET_PATH env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        registry: String,
    },

    /// Login to an OAuth2 provider
    Login {
        /// Issuer base URL, e.g. https://id.example.com/realms/myrealm
        #[arg(long, default_value = "http://localhost:8080/realms/vorpal")]
        issuer: String,

        #[arg(long)]
        /// Issuer OAuth2 Client Audience
        issuer_audience: Option<String>,

        /// Issuer OAuth2 Client ID
        #[arg(long, default_value = "cli")]
        issuer_client_id: String,

        /// Registry address (VORPAL_SOCKET_PATH env var overrides default socket path)
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

        /// Registry address (VORPAL_SOCKET_PATH env var overrides default socket path)
        #[arg(default_value_t = get_default_address(), long)]
        registry: String,
    },

    /// Manage Vorpal system
    #[clap(subcommand)]
    System(CommandSystem),
}

/// h2 resolves a malformed request (e.g. an invalid client `:authority`) by
/// resetting just that stream with PROTOCOL_ERROR entirely inside the h2
/// crate: `Streams::reset_on_recv_stream_err` swallows the reset into
/// `Ok(())` before it ever reaches tonic/tower, so no service, interceptor,
/// or layer in the request-handling stack observes it. The only trace is
/// h2's own `tracing::debug!` ("malformed headers: ..." / "... PROTOCOL_ERROR
/// -- ..."), invisible under the default `--level info`. This layer relays
/// that debug event as a WARN so it's visible without `--level debug`,
/// without touching h2's rejection behavior (DKT-32, diagnosed in DKT-28).
struct H2ProtocolErrorRelay;

struct H2MessageVisitor(String);

impl tracing::field::Visit for H2MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

/// True when a `target`/`level`/`message` triple is h2's own signal for a
/// stream it rejected with PROTOCOL_ERROR (malformed headers, or the
/// `proto_err!`-shaped "stream/connection error PROTOCOL_ERROR -- ..." family).
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

        if is_h2_protocol_error(metadata.target(), *metadata.level(), &message.0) {
            tracing::warn!(target: "vorpal_cli::agent", "h2 rejected stream: {}", message.0);
        }
    }
}

#[cfg(test)]
mod h2_protocol_error_relay_tests {
    use super::*;

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
    fn prepare_unlock_omitted_defaults_true() {
        let cli = parse(&["prepare", "foo"]).expect("should parse");
        match cli.command {
            Command::Prepare { unlock, .. } => assert!(unlock),
            _ => panic!("expected Prepare command"),
        }
    }

    #[test]
    fn prepare_unlock_bare_flag_is_true() {
        let cli = parse(&["prepare", "foo", "--unlock"]).expect("should parse");
        match cli.command {
            Command::Prepare { unlock, .. } => assert!(unlock),
            _ => panic!("expected Prepare command"),
        }
    }

    #[test]
    fn prepare_unlock_equals_false_is_false() {
        let cli = parse(&["prepare", "foo", "--unlock=false"]).expect("should parse");
        match cli.command {
            Command::Prepare { unlock, .. } => assert!(!unlock),
            _ => panic!("expected Prepare command"),
        }
    }

    #[test]
    fn prepare_unlock_equals_true_is_true() {
        let cli = parse(&["prepare", "foo", "--unlock=true"]).expect("should parse");
        match cli.command {
            Command::Prepare { unlock, .. } => assert!(unlock),
            _ => panic!("expected Prepare command"),
        }
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
    fn build_unlock_omitted_defaults_false() {
        let cli = parse(&["build", "foo"]).expect("should parse");
        match cli.command {
            Command::Build { unlock, .. } => assert!(!unlock),
            _ => panic!("expected Build command"),
        }
    }
}

/// If the parsed value matches the hardcoded clap default, substitute the
/// resolved settings value. This ensures explicit CLI flags always win, while
/// config-file values override built-in defaults.
fn apply_default(parsed: &str, clap_default: &str, resolved_value: &str) -> String {
    if parsed == clap_default {
        resolved_value.to_string()
    } else {
        parsed.to_string()
    }
}

/// Shared by `build` and `prepare`: resolves settings/Vorpal.toml fallbacks,
/// then runs the artifact graph through `build::run()`. `prepare_only` gates
/// the early-return (before worker dispatch) added in build.rs; `export`,
/// `list`, `path`, and `rebuild` are always `false` for `prepare` (that flag
/// surface is build-output-specific and not exposed on the `prepare` subcommand).
#[allow(clippy::too_many_arguments)]
async fn run_build_or_prepare(
    resolved: &config::ResolvedSettings,
    project_config: &config::VorpalConfig,
    name: &str,
    agent: &str,
    context: &Path,
    export: bool,
    list: bool,
    namespace: &str,
    path: bool,
    prepare_only: bool,
    rebuild: bool,
    registry: &str,
    system: &str,
    unlock: bool,
    variable: &[String],
    worker: &str,
) -> Result<()> {
    // Apply resolved settings as fallbacks for hardcoded clap defaults
    let default_addr = get_default_address();
    let default_ns = get_default_namespace();

    // Agent is a local service — it should NOT inherit the `registry`
    // setting. Only override it when the user passes an explicit --agent flag.
    let effective_agent = agent.to_string();
    let effective_registry = apply_default(registry, &default_addr, &resolved.registry.value);
    let effective_worker = apply_default(worker, &default_addr, &resolved.worker.value);
    let effective_namespace = apply_default(namespace, &default_ns, &resolved.namespace.value);
    let default_system = get_system_default_str();
    let effective_system = apply_default(system, &default_system, &resolved.system.value);

    if name.is_empty() {
        error!("no name specified");

        exit(1);
    }

    // Use the project config already loaded during resolution

    let config_language = resolved.language.value.clone();
    let config_name = resolved.name.value.clone();

    let mut config_environments = Vec::new();
    let mut config_source_go_directory = None;
    let mut config_source_includes = Vec::new();
    let mut config_source_python_directory = None;
    let mut config_source_python_entrypoint = None;
    let mut config_source_rust_bin = None;
    let mut config_source_rust_packages = None;
    let mut config_source_script = None;
    let mut config_source_typescript_directory = None;
    let mut config_source_typescript_entrypoint = None;

    if let Some(environments) = &project_config.environments {
        if !environments.is_empty() {
            config_environments = environments.clone();
        }
    }

    if let Some(config_source) = &project_config.source {
        if let Some(config_source_go) = &config_source.go {
            if let Some(directory) = &config_source_go.directory {
                if !directory.is_empty() {
                    config_source_go_directory = Some(directory.clone());
                }
            }
        }

        if let Some(includes) = &config_source.includes {
            if !includes.is_empty() {
                config_source_includes = includes.clone();
            }
        }

        if let Some(config_source_python) = &config_source.python {
            if let Some(directory) = &config_source_python.directory {
                if !directory.is_empty() {
                    config_source_python_directory = Some(directory.clone());
                }
            }

            if let Some(entrypoint) = &config_source_python.entrypoint {
                if !entrypoint.is_empty() {
                    config_source_python_entrypoint = Some(entrypoint.clone());
                }
            }
        }

        if let Some(config_source_rust) = &config_source.rust {
            if let Some(ca_source_rust_bin) = &config_source_rust.bin {
                if !ca_source_rust_bin.is_empty() {
                    config_source_rust_bin = Some(ca_source_rust_bin.clone());
                }
            }

            if let Some(packages) = &config_source_rust.packages {
                if !packages.is_empty() {
                    config_source_rust_packages = Some(packages.clone());
                }
            }
        }

        if let Some(script) = &config_source.script {
            if !script.is_empty() {
                config_source_script = Some(script.clone());
            }
        }

        if let Some(config_source_typescript) = &config_source.typescript {
            if let Some(directory) = &config_source_typescript.directory {
                if !directory.is_empty() {
                    config_source_typescript_directory = Some(directory.clone());
                }
            }

            if let Some(entrypoint) = &config_source_typescript.entrypoint {
                if !entrypoint.is_empty() {
                    config_source_typescript_entrypoint = Some(entrypoint.clone());
                }
            }
        }
    };

    // Load project context

    let mut context = context.to_path_buf();

    if !context.is_absolute() {
        let current_dir = current_dir().expect("failed to get current directory");

        context = current_dir.join(context).clean().to_path_buf();
    }

    context = context.clean();

    // Build artifact

    let run_artifact = build::RunArgsArtifact {
        aliases: vec![],
        context: context.clone(),
        export,
        list,
        name: name.to_string(),
        namespace: effective_namespace.clone(),
        path,
        prepare_only,
        rebuild,
        system: effective_system.clone(),
        unlock,
        variable: variable.to_vec(),
    };

    let run_config = build::RunArgsConfig {
        context,
        environments: config_environments,
        language: config_language,
        name: config_name,
        source: Some(VorpalConfigSource {
            go: Some(VorpalConfigSourceGo {
                directory: config_source_go_directory,
            }),
            includes: Some(config_source_includes),
            python: Some(VorpalConfigSourcePython {
                directory: config_source_python_directory,
                entrypoint: config_source_python_entrypoint,
            }),
            rust: Some(VorpalConfigSourceRust {
                bin: config_source_rust_bin,
                packages: config_source_rust_packages,
            }),
            script: config_source_script,
            typescript: Some(VorpalConfigSourceTypeScript {
                directory: config_source_typescript_directory,
                entrypoint: config_source_typescript_entrypoint,
            }),
        }),
    };

    let run_service = build::RunArgsService {
        agent: effective_agent,
        registry: effective_registry,
        worker: effective_worker,
    };

    build::run(run_artifact, run_config, run_service).await
}

pub async fn run() -> Result<()> {
    ring::default_provider()
        .install_default()
        .expect("failed to install ring as default crypto provider");

    let cli = Cli::parse();

    let Cli { command, level } = cli;

    let subscriber_writer = std::io::stderr.with_max_level(level);
    let mut fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(subscriber_writer)
        .without_time();

    if [Level::DEBUG, Level::TRACE].contains(&level) {
        fmt_layer = fmt_layer.with_file(true).with_line_number(true);
    }

    // Per-layer filtering: the main fmt layer stays at the user-selected
    // `--level` (default info), while the h2 relay layer is scoped to the
    // `h2` target at debug so only h2's own debug-level events are enabled -
    // this keeps the process-global max level at `level` instead of raising
    // it to DEBUG for every target. The relayed WARN it emits then flows back
    // through fmt_layer's info-level filter like any other event, so it's
    // visible by default without lowering the general level.
    let subscriber = Registry::default()
        .with(fmt_layer.with_filter(LevelFilter::from_level(level)))
        .with(
            H2ProtocolErrorRelay.with_filter(Targets::new().with_target("h2", LevelFilter::DEBUG)),
        );

    subscriber::set_global_default(subscriber).expect("setting default subscriber");

    // Extract the config path from commands that have one, before resolving settings
    let config_for_settings = match &command {
        Command::Build { config, .. }
        | Command::Config { config, .. }
        | Command::Prepare { config, .. } => config.clone(),
        _ => PathBuf::from("Vorpal.toml"),
    };

    // Resolve layered settings (user config + project config + built-in defaults).
    // If config loading fails (e.g. malformed file), fall back to built-in defaults
    // so that the CLI still works without a valid config.
    let (resolved, project_config) =
        config::resolve_config(&config_for_settings).unwrap_or_else(|_| {
            let defaults = config::VorpalConfig::defaults();
            let resolved = config::ResolvedSettings::resolve(
                &defaults,
                &config::VorpalConfig::default(),
                &config::VorpalConfig::default(),
            );
            (resolved, config::VorpalConfig::default())
        });

    match &command {
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
                name,
                agent,
                context,
                *export,
                *list,
                namespace,
                *path,
                false,
                *rebuild,
                registry,
                system,
                *unlock,
                variable,
                worker,
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
                name,
                agent,
                context,
                false,
                false,
                namespace,
                false,
                true,
                false,
                registry,
                system,
                *unlock,
                variable,
                worker,
            )
            .await
        }

        Command::Config {
            user,
            config,
            action,
        } => match action {
            config_cmd::ConfigAction::Set { key, value } => {
                config_cmd::handle_set(key, value, *user, config)
            }
            config_cmd::ConfigAction::Get { key } => config_cmd::handle_get(key, *user, config),
            config_cmd::ConfigAction::Show => config_cmd::handle_show(config),
        },

        Command::Init { name, path } => init::run(name, path).await,

        Command::Inspect {
            digest,
            namespace,
            registry,
        } => {
            let effective_registry =
                apply_default(registry, &get_default_address(), &resolved.registry.value);
            let effective_namespace = apply_default(
                namespace,
                &get_default_namespace(),
                &resolved.namespace.value,
            );
            inspect::run(digest, &effective_namespace, &effective_registry).await
        }

        Command::Login {
            issuer,
            issuer_audience,
            issuer_client_id,
            registry,
        } => {
            let effective_issuer = issuer.clone();
            let effective_issuer_client_id = issuer_client_id.clone();
            let effective_registry =
                apply_default(registry, &get_default_address(), &resolved.registry.value);

            let discovery_url = format!(
                "{}/.well-known/openid-configuration",
                effective_issuer.trim_end_matches('/')
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

            let client = BasicClient::new(ClientId::new(effective_issuer_client_id.clone()))
                .set_auth_uri(AuthUrl::new(effective_issuer.clone())?)
                .set_token_uri(TokenUrl::new(token_endpoint.to_string())?)
                .set_device_authorization_url(client_device_url);

            let http_client = reqwest::ClientBuilder::new()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Client should build");

            let mut device_request = client
                .exchange_device_code()
                .add_scope(Scope::new("offline_access".to_string()));

            if let Some(audience) = issuer_audience {
                device_request = device_request.add_extra_param("audience", audience);
            }

            let details: StandardDeviceAuthorizationResponse =
                device_request.request_async(&http_client).await?;

            if let Some(complete_uri) = details.verification_uri_complete() {
                println!(
                    "Open this URL in your browser:\n{}",
                    complete_uri.clone().into_secret()
                );
            };

            println!(
                "Or open {} and enter code: {}",
                details.verification_uri(),
                details.user_code().secret()
            );

            let token_result = client
                .exchange_device_access_token(&details)
                .request_async(&http_client, sleep, None)
                .await?;

            let access_token = token_result.access_token().secret();

            let expires_in = token_result
                .expires_in()
                .map(|d| d.as_secs())
                .unwrap_or_default();

            let refresh_token = token_result
                .refresh_token()
                .map(|t| t.secret().to_string())
                .unwrap_or_default();

            let scopes = token_result
                .scopes()
                .map(|s| s.iter().map(|scope| scope.to_string()).collect::<Vec<_>>())
                .unwrap_or_default();

            // Prepare to store token

            let content = VorpalCredentialsContent {
                access_token: access_token.to_string(),
                audience: issuer_audience.clone(),
                client_id: effective_issuer_client_id.clone(),
                expires_in,
                issued_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                refresh_token,
                scopes,
            };

            // TODO: load existing credentials file if it exists

            let mut issuer_map = BTreeMap::new();
            let mut registry_map = BTreeMap::new();

            issuer_map.insert(effective_issuer.clone(), content);
            registry_map.insert(effective_registry.clone(), effective_issuer.clone());

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

        Command::Run {
            alias,
            args,
            bin,
            registry,
        } => {
            let effective_registry =
                apply_default(registry, &get_default_address(), &resolved.registry.value);
            run::run(alias, args, bin.as_deref(), &effective_registry).await
        }

        Command::System(system) => match system {
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
            } => {
                system::prune::run(*all, *aliases, *archives, *configs, *outputs, *sandboxes).await
            }

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
                        archive_cache_ttl: *archive_cache_ttl,
                        health_check: *health_check,
                        health_check_port: *health_check_port,
                        issuer: issuer.clone(),
                        issuer_audience: issuer_audience.clone(),
                        issuer_client_id: issuer_client_id.clone(),
                        issuer_client_secret: issuer_client_secret.clone(),
                        issuer_service_client_ids,
                        port: *port,
                        registry_backend: registry_backend.clone(),
                        registry_backend_s3_bucket: registry_backend_s3_bucket.clone(),
                        registry_backend_s3_force_path_style: *registry_backend_s3_force_path_style,
                        services: services.split(',').map(|s| s.to_string()).collect(),
                        tls: *tls,
                    };

                    start::run(run_args).await
                }
            },
        },
    }
}
