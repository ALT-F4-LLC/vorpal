use crate::command::{
    config::{VorpalConfigSource, VorpalConfigSourceGo, VorpalConfigSourceRust},
    store::paths::get_key_credentials_path,
};
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, DeviceAuthorizationUrl, Scope,
    StandardDeviceAuthorizationResponse, TokenResponse, TokenUrl,
};
use path_clean::PathClean;
use rustls::crypto::ring;
use std::{collections::BTreeMap, env::current_dir, path::PathBuf, process::exit};
use tokio::{fs::write, time::sleep};
use tracing::{error, subscriber, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};
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

        /// TTL in seconds for caching archive check results. Set to 0 to disable caching.
        #[arg(default_value = "300", long)]
        archive_check_cache_ttl: u64,
    },
}

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

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    // Log level
    #[arg(default_value_t = Level::INFO, global = true, long)]
    level: Level,
}

pub async fn run() -> Result<()> {
    ring::default_provider()
        .install_default()
        .expect("failed to install ring as default crypto provider");

    let cli = Cli::parse();

    let Cli { command, level } = cli;

    // Set up tracing subscriber

    let subscriber_writer = std::io::stderr.with_max_level(level);

    let mut subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .with_writer(subscriber_writer)
        .without_time();

    if [Level::DEBUG, Level::TRACE].contains(&level) {
        subscriber = subscriber.with_file(true).with_line_number(true);
    }

    let subscriber = subscriber.finish();

    subscriber::set_global_default(subscriber).expect("setting default subscriber");

    // Extract the config path from commands that have one, before resolving settings
    let config_for_settings = match &command {
        Command::Build { config, .. } | Command::Config { config, .. } => config.clone(),
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

    // Helper: if the parsed value matches the hardcoded clap default, substitute the
    // resolved settings value. This ensures explicit CLI flags always win, while
    // config-file values override built-in defaults.
    let apply_default = |parsed: &str, clap_default: &str, resolved_value: &str| -> String {
        if parsed == clap_default {
            resolved_value.to_string()
        } else {
            parsed.to_string()
        }
    };

    match &command {
        Command::Build {
            agent,
            context,
            export,
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
            // Apply resolved settings as fallbacks for hardcoded clap defaults
            let default_addr = get_default_address();
            let default_ns = get_default_namespace();

            // Agent is a local service — it should NOT inherit the `registry`
            // setting. Only override it when the user passes an explicit --agent flag.
            let effective_agent = agent.to_string();
            let effective_registry =
                apply_default(registry, &default_addr, &resolved.registry.value);
            let effective_worker = apply_default(worker, &default_addr, &resolved.worker.value);
            let effective_namespace =
                apply_default(namespace, &default_ns, &resolved.namespace.value);
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
            let mut config_source_rust_bin = None;
            let mut config_source_rust_packages = None;
            let mut config_source_script = None;

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
                export: *export,
                name: name.clone(),
                namespace: effective_namespace.clone(),
                path: *path,
                rebuild: *rebuild,
                system: effective_system.clone(),
                unlock: *unlock,
                variable: variable.clone(),
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
                    rust: Some(VorpalConfigSourceRust {
                        bin: config_source_rust_bin,
                        packages: config_source_rust_packages,
                    }),
                    script: config_source_script,
                }),
            };

            let run_service = build::RunArgsService {
                agent: effective_agent,
                registry: effective_registry,
                worker: effective_worker,
            };

            build::run(run_artifact, run_config, run_service).await
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

            write(&credentials_path, credentials_json.as_bytes()).await?;

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
                    archive_check_cache_ttl,
                    health_check,
                    health_check_port,
                    issuer,
                    issuer_audience,
                    issuer_client_id,
                    issuer_client_secret,
                    port,
                    registry_backend,
                    registry_backend_s3_bucket,
                    registry_backend_s3_force_path_style,
                    services,
                    tls,
                } => {
                    let run_args = start::RunArgs {
                        archive_check_cache_ttl: *archive_check_cache_ttl,
                        health_check: *health_check,
                        health_check_port: *health_check_port,
                        issuer: issuer.clone(),
                        issuer_audience: issuer_audience.clone(),
                        issuer_client_id: issuer_client_id.clone(),
                        issuer_client_secret: issuer_client_secret.clone(),
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
