use anyhow::{anyhow, bail, Context, Result};
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use port_selector::random_free_port;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process,
    process::Child,
};
use tokio_stream::{wrappers::LinesStream, StreamExt};
use tonic::transport::Channel;
use tracing::{info, warn};
use vorpal_sdk::{
    api::{artifact::Artifact, context::context_service_client::ContextServiceClient},
    artifact::system::get_system_default_str,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VorpalConfigSourceGo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VorpalConfigSourceRust {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packages: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VorpalConfigSourceTypeScript {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VorpalConfigSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub go: Option<VorpalConfigSourceGo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub includes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust: Option<VorpalConfigSourceRust>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typescript: Option<VorpalConfigSourceTypeScript>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VorpalConfig {
    // Settings fields (top-level TOML keys)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,

    // Build config fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environments: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<VorpalConfigSource>,
}

impl VorpalConfig {
    const SETTINGS_FIELD_NAMES: &[&str] = &[
        "registry",
        "namespace",
        "language",
        "name",
        "system",
        "worker",
    ];

    /// Returns the built-in defaults for settings fields.
    pub fn defaults() -> Self {
        Self {
            registry: Some("unix:///var/lib/vorpal/vorpal.sock".to_string()),
            namespace: Some("library".to_string()),
            language: Some("rust".to_string()),
            name: Some("vorpal".to_string()),
            system: Some(get_system_default_str()),
            worker: Some("unix:///var/lib/vorpal/vorpal.sock".to_string()),
            environments: None,
            source: None,
        }
    }

    /// Returns the list of all valid settings key names.
    pub fn field_names() -> &'static [&'static str] {
        Self::SETTINGS_FIELD_NAMES
    }

    /// Set a settings field by its key name.
    pub fn set_by_name(&mut self, name: &str, value: String) -> Result<(), String> {
        match name {
            "registry" => self.registry = Some(value),
            "namespace" => self.namespace = Some(value),
            "language" => self.language = Some(value),
            "name" => self.name = Some(value),
            "system" => self.system = Some(value),
            "worker" => self.worker = Some(value),
            _ => {
                return Err(format!(
                    "unknown setting key '{}'. Valid keys: {}",
                    name,
                    Self::SETTINGS_FIELD_NAMES.join(", ")
                ));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SettingsSource – tracks where a resolved value came from
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SettingsSource {
    Default,
    User,
    Project,
}

impl fmt::Display for SettingsSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::User => write!(f, "user"),
            Self::Project => write!(f, "project"),
        }
    }
}

// ---------------------------------------------------------------------------
// ResolvedValue / ResolvedSettings – fully-merged config with provenance
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ResolvedValue {
    pub value: String,
    pub source: SettingsSource,
}

#[derive(Clone, Debug)]
pub struct ResolvedSettings {
    pub registry: ResolvedValue,
    pub namespace: ResolvedValue,
    pub language: ResolvedValue,
    pub name: ResolvedValue,
    pub system: ResolvedValue,
    pub worker: ResolvedValue,
}

impl ResolvedSettings {
    /// Resolve three layers into a single `ResolvedSettings`.
    ///
    /// Precedence (highest to lowest): `project` > `user` > `defaults`.
    pub fn resolve(defaults: &VorpalConfig, user: &VorpalConfig, project: &VorpalConfig) -> Self {
        fn pick(
            default: &Option<String>,
            user: &Option<String>,
            project: &Option<String>,
        ) -> ResolvedValue {
            if let Some(v) = project {
                ResolvedValue {
                    value: v.clone(),
                    source: SettingsSource::Project,
                }
            } else if let Some(v) = user {
                ResolvedValue {
                    value: v.clone(),
                    source: SettingsSource::User,
                }
            } else if let Some(v) = default {
                ResolvedValue {
                    value: v.clone(),
                    source: SettingsSource::Default,
                }
            } else {
                ResolvedValue {
                    value: String::new(),
                    source: SettingsSource::Default,
                }
            }
        }

        Self {
            registry: pick(&defaults.registry, &user.registry, &project.registry),
            namespace: pick(&defaults.namespace, &user.namespace, &project.namespace),
            language: pick(&defaults.language, &user.language, &project.language),
            name: pick(&defaults.name, &user.name, &project.name),
            system: pick(&defaults.system, &user.system, &project.system),
            worker: pick(&defaults.worker, &user.worker, &project.worker),
        }
    }

    /// Look up a resolved value by its key name.
    pub fn get_by_name(&self, name: &str) -> Option<&ResolvedValue> {
        match name {
            "registry" => Some(&self.registry),
            "namespace" => Some(&self.namespace),
            "language" => Some(&self.language),
            "name" => Some(&self.name),
            "system" => Some(&self.system),
            "worker" => Some(&self.worker),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// File I/O – loading and saving config from config files
// ---------------------------------------------------------------------------

/// Returns the path to the user-level settings file.
pub fn get_user_config_path() -> PathBuf {
    let base = if let Ok(dir) = std::env::var("VORPAL_USER_CONFIG_DIR") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".vorpal")
    };
    base.join("settings.json")
}

/// Load user-level config from a JSON file.
///
/// Returns `VorpalConfig::default()` (all `None`) if the file does not exist.
pub fn load_user_config(path: &Path) -> Result<VorpalConfig> {
    if !path.exists() {
        return Ok(VorpalConfig::default());
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read user config from {}", path.display()))?;
    let config: VorpalConfig = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse user config from {}", path.display()))?;
    Ok(config)
}

/// Load project-level config from a `Vorpal.toml` file.
///
/// Parses the entire TOML file as `VorpalConfig`. Returns `VorpalConfig::default()`
/// if the file does not exist.
pub fn load_project_config(path: &Path) -> Result<VorpalConfig> {
    if !path.exists() {
        return Ok(VorpalConfig::default());
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read project config from {}", path.display()))?;
    let config: VorpalConfig = toml::from_str(&contents)
        .with_context(|| format!("failed to parse TOML from {}", path.display()))?;
    Ok(config)
}

/// Save user-level config to a JSON file.
///
/// Creates parent directories if they do not exist.
pub fn save_user_config(path: &Path, config: &VorpalConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    let json =
        serde_json::to_string_pretty(config).context("failed to serialize user config to JSON")?;
    fs::write(path, json)
        .with_context(|| format!("failed to write user config to {}", path.display()))?;
    Ok(())
}

/// Save a single key-value pair to a project-level `Vorpal.toml` file.
///
/// Operates at the `toml::Table` level to avoid overwriting `source`/`environments`.
pub fn save_project_config(path: &Path, key: &str, value: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    let mut table: toml::Table = if path.exists() {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("failed to parse TOML from {}", path.display()))?
    } else {
        toml::Table::new()
    };

    table.insert(key.to_string(), toml::Value::String(value.to_string()));

    let toml_str = toml::to_string_pretty(&table).context("failed to serialize Vorpal.toml")?;
    fs::write(path, toml_str).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Resolve all config layers and return `(ResolvedSettings, VorpalConfig)`.
///
/// The returned `VorpalConfig` is the project-level config, which includes
/// `environments` and `source` needed by the build command.
pub fn resolve_config(config_path: &Path) -> Result<(ResolvedSettings, VorpalConfig)> {
    let defaults = VorpalConfig::defaults();
    let user_path = get_user_config_path();
    let user = load_user_config(&user_path)?;
    let project = load_project_config(config_path)?;
    let resolved = ResolvedSettings::resolve(&defaults, &user, &project);
    Ok((resolved, project))
}

pub async fn get_artifacts(
    artifact: &Artifact,
    artifact_digest: &str,
    build_store: &mut HashMap<String, Artifact>,
    config_store: &HashMap<String, Artifact>,
) -> Result<()> {
    if !build_store.contains_key(artifact_digest) {
        build_store.insert(artifact_digest.to_string(), artifact.clone());
    }

    for step in artifact.steps.iter() {
        for artifact_digest in step.artifacts.iter() {
            if build_store.contains_key(artifact_digest) {
                continue;
            }

            let artifact = config_store
                .get(artifact_digest)
                .ok_or_else(|| anyhow!("artifact 'config' not found: {}", artifact_digest))?;

            build_store.insert(artifact_digest.to_string(), artifact.clone());

            Box::pin(get_artifacts(
                artifact,
                artifact_digest,
                build_store,
                config_store,
            ))
            .await?
        }
    }

    Ok(())
}

pub async fn get_order(config_artifact: &HashMap<String, Artifact>) -> Result<Vec<String>> {
    let mut artifact_graph = DiGraphMap::<&String, Artifact>::new();

    for (artifact_hash, artifact) in config_artifact.iter() {
        artifact_graph.add_node(artifact_hash);

        for step in artifact.steps.iter() {
            for step_artifact_hash in step.artifacts.iter() {
                artifact_graph.add_edge(step_artifact_hash, artifact_hash, artifact.clone());
            }
        }
    }

    let build_order = match toposort(&artifact_graph, None) {
        Err(err) => bail!("{:?}", err),
        Ok(order) => order,
    };

    let build_order: Vec<String> = build_order.into_iter().cloned().collect();

    Ok(build_order)
}

#[allow(clippy::too_many_arguments)]
pub async fn start(
    agent: String,
    artifact_context: PathBuf,
    artifact_name: String,
    artifact_namespace: String,
    artifact_system: String,
    artifact_unlock: bool,
    artifact_variable: Vec<String>,
    config_file: String,
    registry: String,
) -> Result<(Child, ContextServiceClient<Channel>)> {
    let command_artifact_context = artifact_context.display().to_string();
    let command_port = random_free_port().ok_or_else(|| anyhow!("failed to find free port"))?;
    let command_port = command_port.to_string();

    let mut command = process::Command::new(config_file.clone());

    let command_arguments = vec![
        "start",
        "--agent",
        &agent,
        "--artifact",
        &artifact_name,
        "--artifact-context",
        &command_artifact_context,
        "--artifact-namespace",
        &artifact_namespace,
        "--artifact-system",
        &artifact_system,
        "--port",
        &command_port,
        "--registry",
        &registry,
    ];

    command.args(command_arguments);

    if artifact_unlock {
        command.arg("--artifact-unlock");
    }

    for var in artifact_variable.iter() {
        command.arg("--artifact-variable").arg(var);
    }

    let mut config_process = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| anyhow!("failed to start config server"))?;

    let stdout = config_process.stdout.take().unwrap();
    let stderr = config_process.stderr.take().unwrap();

    let stdout = LinesStream::new(BufReader::new(stdout).lines());
    let stderr = LinesStream::new(BufReader::new(stderr).lines());

    let mut stdio_merged = StreamExt::merge(stdout, stderr);

    loop {
        match stdio_merged.next().await {
            Some(Ok(line)) => {
                if line.contains("context service:") {
                    break;
                }

                if line.starts_with("Error: ") {
                    let _ = config_process
                        .kill()
                        .await
                        .map_err(|_| anyhow!("failed to kill config server"));

                    bail!("{}", line.replace("Error: ", ""));
                }

                info!("{}", line);
            }

            Some(Err(err)) => {
                let _ = config_process
                    .kill()
                    .await
                    .map_err(|_| anyhow!("failed to kill config server"));

                bail!("failed to read line: {:?}", err);
            }

            None => break,
        }
    }

    let config_host = format!("http://localhost:{command_port}");

    let mut attempts = 0;
    let max_attempts = 3;
    let max_wait_time = Duration::from_millis(500);

    let config_client = loop {
        attempts += 1;

        match ContextServiceClient::connect(config_host.clone()).await {
            Ok(srv) => break srv,
            Err(e) => {
                if attempts >= max_attempts {
                    let _ = config_process
                        .kill()
                        .await
                        .map_err(|_| anyhow!("failed to kill config server"));

                    bail!("failed to connect after {} attempts: {}", max_attempts, e);
                }

                warn!(
                    "context client {}/{} failed, retry in {} ms...",
                    attempts,
                    max_attempts,
                    max_wait_time.as_millis()
                );

                tokio::time::sleep(max_wait_time).await;
            }
        }
    };

    Ok((config_process, config_client))
}
