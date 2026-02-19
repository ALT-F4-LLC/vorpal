use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Settings – the shared configuration schema
// ---------------------------------------------------------------------------

/// Layered configuration schema.
///
/// Every field is `Option<T>` so that a partial layer (user or project) only
/// carries the keys that were explicitly set. `None` means "not specified in
/// this layer".
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer_client_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Settings {
    /// All valid setting key names.
    const FIELD_NAMES: &[&str] = &[
        "registry",
        "namespace",
        "language",
        "issuer",
        "issuer_client_id",
        "name",
    ];

    /// Returns the built-in defaults for every setting.
    pub fn defaults() -> Self {
        Self {
            registry: Some("unix:///var/lib/vorpal/vorpal.sock".to_string()),
            namespace: Some("library".to_string()),
            language: Some("rust".to_string()),
            issuer: Some("http://localhost:8080/realms/vorpal".to_string()),
            issuer_client_id: Some("cli".to_string()),
            name: Some("vorpal".to_string()),
        }
    }

    /// Merge two layers. Values in `other` override values in `self`.
    ///
    /// If a field is `Some` in `other`, the result uses `other`'s value;
    /// otherwise it keeps `self`'s value.
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            registry: other.registry.clone().or_else(|| self.registry.clone()),
            namespace: other.namespace.clone().or_else(|| self.namespace.clone()),
            language: other.language.clone().or_else(|| self.language.clone()),
            issuer: other.issuer.clone().or_else(|| self.issuer.clone()),
            issuer_client_id: other
                .issuer_client_id
                .clone()
                .or_else(|| self.issuer_client_id.clone()),
            name: other.name.clone().or_else(|| self.name.clone()),
        }
    }

    /// Returns the list of all valid setting key names.
    pub fn field_names() -> &'static [&'static str] {
        Self::FIELD_NAMES
    }

    /// Look up a setting value by its key name.
    pub fn get_by_name(&self, name: &str) -> Option<&String> {
        match name {
            "registry" => self.registry.as_ref(),
            "namespace" => self.namespace.as_ref(),
            "language" => self.language.as_ref(),
            "issuer" => self.issuer.as_ref(),
            "issuer_client_id" => self.issuer_client_id.as_ref(),
            "name" => self.name.as_ref(),
            _ => None,
        }
    }

    /// Set a setting value by its key name.
    ///
    /// Returns `Err` with a message if `name` is not a recognized key.
    pub fn set_by_name(&mut self, name: &str, value: String) -> Result<(), String> {
        match name {
            "registry" => self.registry = Some(value),
            "namespace" => self.namespace = Some(value),
            "language" => self.language = Some(value),
            "issuer" => self.issuer = Some(value),
            "issuer_client_id" => self.issuer_client_id = Some(value),
            "name" => self.name = Some(value),
            _ => {
                return Err(format!(
                    "unknown setting key '{}'. Valid keys: {}",
                    name,
                    Self::FIELD_NAMES.join(", ")
                ));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SettingsSource – tracks where a resolved value came from
// ---------------------------------------------------------------------------

/// Identifies which configuration layer provided a resolved value.
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

/// A single resolved configuration value paired with its source layer.
#[derive(Clone, Debug)]
pub struct ResolvedValue {
    pub value: String,
    pub source: SettingsSource,
}

/// The fully-resolved configuration after merging all layers.
///
/// Every field is guaranteed to have a value (there are no `Option`s) because
/// the built-in defaults provide a value for every key.
#[derive(Clone, Debug)]
pub struct ResolvedSettings {
    pub registry: ResolvedValue,
    pub namespace: ResolvedValue,
    pub language: ResolvedValue,
    pub issuer: ResolvedValue,
    pub issuer_client_id: ResolvedValue,
    pub name: ResolvedValue,
}

impl ResolvedSettings {
    /// Resolve three layers into a single `ResolvedSettings`.
    ///
    /// Precedence (highest to lowest): `project` > `user` > `defaults`.
    pub fn resolve(defaults: &Settings, user: &Settings, project: &Settings) -> Self {
        /// Pick the highest-precedence `Some` value, tracking its source.
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
                // Built-in defaults always provide a value, so this should be
                // unreachable when called with Settings::defaults().
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
            issuer: pick(&defaults.issuer, &user.issuer, &project.issuer),
            issuer_client_id: pick(
                &defaults.issuer_client_id,
                &user.issuer_client_id,
                &project.issuer_client_id,
            ),
            name: pick(&defaults.name, &user.name, &project.name),
        }
    }

    /// Look up a resolved value by its key name.
    pub fn get_by_name(&self, name: &str) -> Option<&ResolvedValue> {
        match name {
            "registry" => Some(&self.registry),
            "namespace" => Some(&self.namespace),
            "language" => Some(&self.language),
            "issuer" => Some(&self.issuer),
            "issuer_client_id" => Some(&self.issuer_client_id),
            "name" => Some(&self.name),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// File I/O – loading and saving settings from config files
// ---------------------------------------------------------------------------

/// Returns the path to the user-level settings file.
///
/// Defaults to `~/.vorpal/settings.json`. The base directory can be overridden
/// by setting the `VORPAL_USER_CONFIG_DIR` environment variable (useful for
/// testing).
pub fn get_user_settings_path() -> PathBuf {
    let base = if let Ok(dir) = std::env::var("VORPAL_USER_CONFIG_DIR") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".vorpal")
    };
    base.join("settings.json")
}

/// Load user-level settings from a JSON file.
///
/// Returns `Settings::default()` (all `None`) if the file does not exist.
pub fn load_user_settings(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read user settings from {}", path.display()))?;
    let settings: Settings = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse user settings from {}", path.display()))?;
    Ok(settings)
}

/// Load project-level settings from a `Vorpal.toml` file.
///
/// Reads the file at `path`, extracts the `[settings]` table, and
/// deserializes it into `Settings`. Returns `Settings::default()` if the file
/// does not exist or the `[settings]` section is absent.
pub fn load_project_settings(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read project settings from {}", path.display()))?;
    let table: toml::Table = toml::from_str(&contents)
        .with_context(|| format!("failed to parse TOML from {}", path.display()))?;

    match table.get("settings") {
        Some(settings_value) => {
            let settings: Settings = settings_value
                .clone()
                .try_into()
                .with_context(|| "failed to deserialize [settings] table")?;
            Ok(settings)
        }
        None => Ok(Settings::default()),
    }
}

/// Save user-level settings to a JSON file.
///
/// Creates parent directories if they do not exist.
pub fn save_user_settings(path: &Path, settings: &Settings) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(settings)
        .context("failed to serialize user settings to JSON")?;
    fs::write(path, json)
        .with_context(|| format!("failed to write user settings to {}", path.display()))?;
    Ok(())
}

/// Save project-level settings to a `Vorpal.toml` file.
///
/// Reads the existing file (if any), updates or inserts the `[settings]`
/// section, and writes the result back. All other tables and keys in the
/// file are preserved.
pub fn save_project_settings(path: &Path, settings: &Settings) -> Result<()> {
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

    // Serialize Settings into a toml::Value, then insert it under "settings".
    let settings_value =
        toml::Value::try_from(settings).context("failed to serialize settings to TOML value")?;
    table.insert("settings".to_string(), settings_value);

    let toml_str = toml::to_string_pretty(&table).context("failed to serialize Vorpal.toml")?;
    fs::write(path, toml_str).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Convenience function: load all layers and resolve them.
///
/// Loads built-in defaults, user settings from `get_user_settings_path()`,
/// and project settings from `./Vorpal.toml` in the current directory.
pub fn resolve_settings() -> Result<ResolvedSettings> {
    let defaults = Settings::defaults();
    let user_path = get_user_settings_path();
    let user = load_user_settings(&user_path)?;

    let project_path = PathBuf::from("Vorpal.toml");
    let project = load_project_settings(&project_path)?;

    Ok(ResolvedSettings::resolve(&defaults, &user, &project))
}
