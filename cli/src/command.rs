use crate::command::{
    config::{VorpalConfigSource, VorpalConfigSourceGo, VorpalConfigSourceRust},
    credentials::{VorpalCredentials, VorpalCredentialsContent},
    store::paths::get_key_credentials_path,
};
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use config::VorpalConfig;
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, DeviceAuthorizationUrl, Scope,
    StandardDeviceAuthorizationResponse, TokenResponse, TokenUrl,
};
use path_clean::PathClean;
use rustls::crypto::ring;
use std::{collections::HashMap, env::current_dir, path::PathBuf, process::exit};
use tokio::{
    fs::{read, write},
    time::sleep,
};
use toml::from_str;
use tracing::{error, subscriber, Level};
use tracing_subscriber::{fmt::writer::MakeWriterExt, FmtSubscriber};
use vorpal_sdk::artifact::{get_default_address, system::get_system_default_str};

mod artifact;
mod config;
mod credentials;
mod init;
mod lock;
mod start;
mod store;
mod system;

pub fn get_default_namespace() -> String {
    "library".to_string()
}

#[derive(Subcommand)]
pub enum CommandArtifact {
    Init {},

    Inspect {
        /// Artifact digest
        digest: String,

        /// Artifact namespace
        #[arg(default_value_t = get_default_namespace(), long)]
        namespace: String,

        /// Registry address
        #[arg(default_value_t = get_default_address(), long)]
        registry: String,
    },

    Make {
        /// Artifact name
        name: String,

        /// Artifact agent address
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

        // Registry address
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

        /// Artifact worker address
        #[arg(default_value_t = get_default_address(), long)]
        worker: String,
    },
}

#[derive(Subcommand)]
pub enum CommandAuth {
    Login {
        /// OAuth2 client_id configured in Keycloak
        #[arg(long, default_value = "cli")]
        client_id: String,

        /// Issuer base URL, e.g. https://id.example.com/realms/myrealm
        #[arg(long, default_value = "http://localhost:8080/realms/vorpal")]
        issuer: String,

        // Registry address
        #[arg(default_value_t = get_default_address(), global = true, long)]
        registry: String,
    },
}

#[derive(Subcommand)]
pub enum CommandServices {
    Start {
        #[arg(long)]
        issuer: Option<String>,

        #[arg(long)]
        issuer_client_id: Option<String>,

        #[arg(long)]
        issuer_client_secret: Option<String>,

        #[arg(default_value = "23151", long)]
        port: u16,

        #[arg(default_value = "agent,registry,worker", long)]
        services: String,

        #[arg(default_value = "local", long)]
        registry_backend: String,

        #[arg(long)]
        registry_backend_s3_bucket: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum CommandSystemKeys {
    Generate {},
}

#[derive(Subcommand)]
pub enum CommandSystem {
    #[clap(subcommand)]
    Keys(CommandSystemKeys),

    Prune {
        #[arg(default_value_t = false, long)]
        all: bool,

        #[arg(long)]
        artifact_aliases: bool,

        #[arg(long)]
        artifact_archives: bool,

        #[arg(long)]
        artifact_configs: bool,

        #[arg(long)]
        artifact_outputs: bool,

        #[arg(long)]
        sandboxes: bool,
    },
}

#[derive(Subcommand)]
pub enum Command {
    #[clap(subcommand)]
    Artifact(CommandArtifact),

    #[clap(subcommand)]
    Auth(CommandAuth),

    #[clap(subcommand)]
    Services(CommandServices),

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

    match &command {
        Command::Artifact(artifact) => match artifact {
            CommandArtifact::Init {} => init::run().await,

            CommandArtifact::Inspect {
                digest,
                namespace,
                registry,
            } => artifact::inspect::run(digest, namespace, registry).await,

            CommandArtifact::Make {
                agent,
                config,
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
            } => {
                if name.is_empty() {
                    error!("no name specified");

                    exit(1);
                }

                // Set default configurations

                let mut config_language = "rust".to_string();
                let mut config_name = "vorpal".to_string();
                let mut config_source_go_directory = None;
                let mut config_source_includes = Vec::new();
                let mut config_source_rust_bin = None;
                let mut config_source_rust_packages = None;

                // Load project configuration

                let mut config_path = config.to_path_buf();

                if !config.is_absolute() {
                    let current_dir = current_dir().expect("failed to get current directory");

                    config_path = current_dir.join(config).clean().to_path_buf();
                }

                config_path = config_path.clean();

                // Load project configuration value, if exists

                let config = match read(&config_path).await {
                    Err(e) => Err(anyhow!("Failed to read {}: {}", config_path.display(), e)),

                    Ok(toml_bytes) => {
                        let toml_str = String::from_utf8_lossy(&toml_bytes);

                        match from_str::<VorpalConfig>(&toml_str) {
                            Err(e) => Err(anyhow!("Failed to parse: {}", e)),
                            Ok(toml) => Ok(toml),
                        }
                    }
                }?;

                if let Some(language) = config.language {
                    config_language = language;
                }

                if let Some(name) = config.name {
                    if !name.is_empty() {
                        config_name = name;
                    }
                }

                if let Some(config_source) = config.source {
                    if let Some(config_source_go) = config_source.go {
                        if let Some(directory) = config_source_go.directory {
                            if !directory.is_empty() {
                                config_source_go_directory = Some(directory);
                            }
                        }
                    }

                    if let Some(includes) = config_source.includes {
                        if !includes.is_empty() {
                            config_source_includes = includes;
                        }
                    }

                    if let Some(config_source_rust) = config_source.rust {
                        if let Some(ca_source_rust_bin) = config_source_rust.bin {
                            if !ca_source_rust_bin.is_empty() {
                                config_source_rust_bin = Some(ca_source_rust_bin);
                            }
                        }

                        if let Some(packages) = config_source_rust.packages {
                            if !packages.is_empty() {
                                config_source_rust_packages = Some(packages);
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

                let run_artifact = artifact::make::RunArgsArtifact {
                    aliases: vec![],
                    context: context.clone(),
                    export: *export,
                    name: name.clone(),
                    namespace: namespace.clone(),
                    path: *path,
                    rebuild: *rebuild,
                    system: system.clone(),
                    unlock: *unlock,
                    variable: variable.clone(),
                };

                let run_config = artifact::make::RunArgsConfig {
                    context,
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
                    }),
                };

                let run_service = artifact::make::RunArgsService {
                    agent: agent.to_string(),
                    registry: registry.to_string(),
                    worker: worker.to_string(),
                };

                artifact::make::run(run_artifact, run_config, run_service).await
            }
        },

        Command::Auth(auth) => match auth {
            CommandAuth::Login {
                client_id,
                issuer,
                registry,
            } => {
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

                let client = BasicClient::new(ClientId::new(client_id.to_string()))
                    .set_auth_uri(AuthUrl::new(issuer.to_string())?)
                    .set_token_uri(TokenUrl::new(token_endpoint.to_string())?)
                    .set_device_authorization_url(client_device_url);

                let http_client = reqwest::ClientBuilder::new()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("Client should build");

                let details: StandardDeviceAuthorizationResponse = client
                    .exchange_device_code()
                    .add_scope(Scope::new("archive".to_string()))
                    .add_scope(Scope::new("artifact".to_string()))
                    .add_scope(Scope::new("worker".to_string()))
                    .request_async(&http_client)
                    .await?;

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
                    expires_in,
                    refresh_token,
                    scopes,
                };

                // TODO: load existing credentials file if it exists

                let mut issuer_map = HashMap::new();
                let mut registry_map = HashMap::new();

                issuer_map.insert(issuer.to_string(), content);
                registry_map.insert(registry.to_string(), issuer.to_string());

                let credentials = VorpalCredentials {
                    issuer: issuer_map,
                    registry: registry_map,
                };
                let credentials_json = serde_json::to_string_pretty(&credentials)?;
                let credentials_path = get_key_credentials_path();

                write(&credentials_path, credentials_json.as_bytes()).await?;

                Ok(())
            }
        },

        Command::Services(services) => match services {
            CommandServices::Start {
                issuer,
                issuer_client_id,
                issuer_client_secret,
                port,
                registry_backend,
                registry_backend_s3_bucket,
                services,
            } => {
                start::run(
                    issuer.clone(),
                    issuer_client_id.clone(),
                    issuer_client_secret.clone(),
                    *port,
                    registry_backend.clone(),
                    registry_backend_s3_bucket.clone(),
                    services.split(',').map(|s| s.to_string()).collect(),
                )
                .await
            }
        },

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
        },
    }
}
