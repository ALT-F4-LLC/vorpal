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
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create directory {}",
                parent.display()
            )
        })?;
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
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create directory {}",
                parent.display()
            )
        })?;
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
    let settings_value = toml::Value::try_from(settings)
        .context("failed to serialize settings to TOML value")?;
    table.insert("settings".to_string(), settings_value);

    let toml_str =
        toml::to_string_pretty(&table).context("failed to serialize Vorpal.toml")?;
    fs::write(path, toml_str)
        .with_context(|| format!("failed to write {}", path.display()))?;
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_returns_all_some() {
        let d = Settings::defaults();
        assert!(d.registry.is_some());
        assert!(d.namespace.is_some());
        assert!(d.language.is_some());
        assert!(d.issuer.is_some());
        assert!(d.issuer_client_id.is_some());
        assert!(d.name.is_some());

        assert_eq!(
            d.registry.as_deref(),
            Some("unix:///var/lib/vorpal/vorpal.sock")
        );
        assert_eq!(d.namespace.as_deref(), Some("library"));
        assert_eq!(d.language.as_deref(), Some("rust"));
        assert_eq!(
            d.issuer.as_deref(),
            Some("http://localhost:8080/realms/vorpal")
        );
        assert_eq!(d.issuer_client_id.as_deref(), Some("cli"));
        assert_eq!(d.name.as_deref(), Some("vorpal"));
    }

    #[test]
    fn merge_partial_override() {
        let base = Settings::defaults();
        let partial = Settings {
            language: Some("go".to_string()),
            ..Default::default()
        };

        let merged = base.merge(&partial);

        // Overridden field
        assert_eq!(merged.language.as_deref(), Some("go"));

        // Fields that should remain from base
        assert_eq!(
            merged.registry.as_deref(),
            Some("unix:///var/lib/vorpal/vorpal.sock")
        );
        assert_eq!(merged.namespace.as_deref(), Some("library"));
        assert_eq!(
            merged.issuer.as_deref(),
            Some("http://localhost:8080/realms/vorpal")
        );
        assert_eq!(merged.issuer_client_id.as_deref(), Some("cli"));
    }

    #[test]
    fn merge_full_override() {
        let base = Settings::defaults();
        let full = Settings {
            registry: Some("https://custom.example.com".to_string()),
            namespace: Some("custom-ns".to_string()),
            language: Some("python".to_string()),
            issuer: Some("https://auth.example.com".to_string()),
            issuer_client_id: Some("my-app".to_string()),
            name: Some("custom-project".to_string()),
        };

        let merged = base.merge(&full);

        assert_eq!(
            merged.registry.as_deref(),
            Some("https://custom.example.com")
        );
        assert_eq!(merged.namespace.as_deref(), Some("custom-ns"));
        assert_eq!(merged.language.as_deref(), Some("python"));
        assert_eq!(
            merged.issuer.as_deref(),
            Some("https://auth.example.com")
        );
        assert_eq!(merged.issuer_client_id.as_deref(), Some("my-app"));
        assert_eq!(merged.name.as_deref(), Some("custom-project"));
    }

    #[test]
    fn serialization_roundtrip_json() {
        let original = Settings::defaults();
        let json = serde_json::to_string_pretty(&original).expect("serialize to JSON");
        let deserialized: Settings =
            serde_json::from_str(&json).expect("deserialize from JSON");

        assert_eq!(original.registry, deserialized.registry);
        assert_eq!(original.namespace, deserialized.namespace);
        assert_eq!(original.language, deserialized.language);
        assert_eq!(original.issuer, deserialized.issuer);
        assert_eq!(original.issuer_client_id, deserialized.issuer_client_id);
        assert_eq!(original.name, deserialized.name);
    }

    #[test]
    fn serialization_roundtrip_toml() {
        let original = Settings::defaults();
        let toml_str = toml::to_string_pretty(&original).expect("serialize to TOML");
        let deserialized: Settings =
            toml::from_str(&toml_str).expect("deserialize from TOML");

        assert_eq!(original.registry, deserialized.registry);
        assert_eq!(original.namespace, deserialized.namespace);
        assert_eq!(original.language, deserialized.language);
        assert_eq!(original.issuer, deserialized.issuer);
        assert_eq!(original.issuer_client_id, deserialized.issuer_client_id);
        assert_eq!(original.name, deserialized.name);
    }

    #[test]
    fn serialization_skips_none_fields_json() {
        let partial = Settings {
            language: Some("go".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&partial).expect("serialize to JSON");
        assert!(json.contains("language"));
        assert!(!json.contains("registry"));
        assert!(!json.contains("namespace"));
        assert!(!json.contains("issuer_client_id"));
    }

    #[test]
    fn resolved_settings_precedence() {
        let defaults = Settings::defaults();
        let user = Settings {
            language: Some("go".to_string()),
            namespace: Some("user-ns".to_string()),
            ..Default::default()
        };
        let project = Settings {
            language: Some("python".to_string()),
            ..Default::default()
        };

        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        // Project overrides user for language
        assert_eq!(resolved.language.value, "python");
        assert_eq!(resolved.language.source, SettingsSource::Project);

        // User overrides default for namespace
        assert_eq!(resolved.namespace.value, "user-ns");
        assert_eq!(resolved.namespace.source, SettingsSource::User);

        // Default used for registry (nothing overrides it)
        assert_eq!(resolved.registry.value, "unix:///var/lib/vorpal/vorpal.sock");
        assert_eq!(resolved.registry.source, SettingsSource::Default);

        // Default used for issuer
        assert_eq!(resolved.issuer.source, SettingsSource::Default);

        // Default used for issuer_client_id
        assert_eq!(resolved.issuer_client_id.source, SettingsSource::Default);
    }

    #[test]
    fn get_by_name_returns_correct_values() {
        let s = Settings::defaults();
        assert_eq!(
            s.get_by_name("registry").map(String::as_str),
            Some("unix:///var/lib/vorpal/vorpal.sock")
        );
        assert_eq!(
            s.get_by_name("namespace").map(String::as_str),
            Some("library")
        );
        assert_eq!(
            s.get_by_name("language").map(String::as_str),
            Some("rust")
        );
        assert_eq!(
            s.get_by_name("issuer").map(String::as_str),
            Some("http://localhost:8080/realms/vorpal")
        );
        assert_eq!(
            s.get_by_name("issuer_client_id").map(String::as_str),
            Some("cli")
        );
        assert_eq!(
            s.get_by_name("name").map(String::as_str),
            Some("vorpal")
        );
        assert!(s.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn set_by_name_sets_correct_fields() {
        let mut s = Settings::default();
        assert!(s.registry.is_none());

        s.set_by_name("registry", "https://example.com".to_string())
            .expect("set registry");
        assert_eq!(s.registry.as_deref(), Some("https://example.com"));

        s.set_by_name("namespace", "my-ns".to_string())
            .expect("set namespace");
        assert_eq!(s.namespace.as_deref(), Some("my-ns"));

        s.set_by_name("language", "go".to_string())
            .expect("set language");
        assert_eq!(s.language.as_deref(), Some("go"));

        s.set_by_name("issuer", "https://auth.example.com".to_string())
            .expect("set issuer");
        assert_eq!(s.issuer.as_deref(), Some("https://auth.example.com"));

        s.set_by_name("issuer_client_id", "my-app".to_string())
            .expect("set issuer_client_id");
        assert_eq!(s.issuer_client_id.as_deref(), Some("my-app"));

        s.set_by_name("name", "my-project".to_string())
            .expect("set name");
        assert_eq!(s.name.as_deref(), Some("my-project"));
    }

    #[test]
    fn set_by_name_rejects_unknown_key() {
        let mut s = Settings::default();
        let result = s.set_by_name("nonexistent", "value".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("unknown setting key"));
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn resolved_get_by_name() {
        let defaults = Settings::defaults();
        let user = Settings::default();
        let project = Settings::default();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        let reg = resolved.get_by_name("registry").expect("registry exists");
        assert_eq!(reg.value, "unix:///var/lib/vorpal/vorpal.sock");
        assert_eq!(reg.source, SettingsSource::Default);

        assert!(resolved.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn settings_source_display() {
        assert_eq!(SettingsSource::Default.to_string(), "default");
        assert_eq!(SettingsSource::User.to_string(), "user");
        assert_eq!(SettingsSource::Project.to_string(), "project");
    }

    #[test]
    fn field_names_lists_all_keys() {
        let names = Settings::field_names();
        assert_eq!(names.len(), 6);
        assert!(names.contains(&"registry"));
        assert!(names.contains(&"namespace"));
        assert!(names.contains(&"language"));
        assert!(names.contains(&"issuer"));
        assert!(names.contains(&"issuer_client_id"));
        assert!(names.contains(&"name"));
    }

    // -----------------------------------------------------------------------
    // File I/O tests
    // -----------------------------------------------------------------------

    #[test]
    fn load_user_settings_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let settings = load_user_settings(&path).unwrap();
        assert!(settings.registry.is_none());
        assert!(settings.namespace.is_none());
        assert!(settings.language.is_none());
        assert!(settings.issuer.is_none());
        assert!(settings.issuer_client_id.is_none());
        assert!(settings.name.is_none());
    }

    #[test]
    fn load_user_settings_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let json = r#"{"registry": "https://example.com", "language": "go"}"#;
        fs::write(&path, json).unwrap();

        let settings = load_user_settings(&path).unwrap();
        assert_eq!(settings.registry.as_deref(), Some("https://example.com"));
        assert_eq!(settings.language.as_deref(), Some("go"));
        assert!(settings.namespace.is_none());
        assert!(settings.issuer.is_none());
        assert!(settings.issuer_client_id.is_none());
    }

    #[test]
    fn save_and_reload_user_settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("settings.json");

        let original = Settings {
            registry: Some("https://test.example.com".to_string()),
            namespace: Some("my-ns".to_string()),
            language: None,
            issuer: Some("https://auth.test.com".to_string()),
            issuer_client_id: None,
            name: None,
        };

        save_user_settings(&path, &original).unwrap();
        assert!(path.exists());

        let reloaded = load_user_settings(&path).unwrap();
        assert_eq!(reloaded.registry, original.registry);
        assert_eq!(reloaded.namespace, original.namespace);
        assert_eq!(reloaded.language, original.language);
        assert_eq!(reloaded.issuer, original.issuer);
        assert_eq!(reloaded.issuer_client_id, original.issuer_client_id);
        assert_eq!(reloaded.name, original.name);
    }

    #[test]
    fn load_project_settings_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Vorpal.toml");
        let settings = load_project_settings(&path).unwrap();
        assert!(settings.registry.is_none());
        assert!(settings.namespace.is_none());
    }

    #[test]
    fn load_project_settings_no_settings_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Vorpal.toml");
        let toml_content = r#"
language = "rust"
name = "my-project"

[source]
includes = ["src"]
"#;
        fs::write(&path, toml_content).unwrap();

        let settings = load_project_settings(&path).unwrap();
        assert!(settings.registry.is_none());
        assert!(settings.language.is_none());
    }

    #[test]
    fn load_project_settings_with_settings_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Vorpal.toml");
        let toml_content = r#"
language = "rust"
name = "my-project"

[settings]
registry = "https://project.example.com"
namespace = "proj-ns"

[source]
includes = ["src"]
"#;
        fs::write(&path, toml_content).unwrap();

        let settings = load_project_settings(&path).unwrap();
        assert_eq!(
            settings.registry.as_deref(),
            Some("https://project.example.com")
        );
        assert_eq!(settings.namespace.as_deref(), Some("proj-ns"));
        assert!(settings.language.is_none());
        assert!(settings.issuer.is_none());
    }

    #[test]
    fn save_and_reload_project_settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Vorpal.toml");

        let original = Settings {
            registry: Some("https://proj.example.com".to_string()),
            namespace: None,
            language: Some("go".to_string()),
            issuer: None,
            issuer_client_id: Some("my-client".to_string()),
            name: Some("test-proj".to_string()),
        };

        save_project_settings(&path, &original).unwrap();
        assert!(path.exists());

        let reloaded = load_project_settings(&path).unwrap();
        assert_eq!(reloaded.registry, original.registry);
        assert_eq!(reloaded.namespace, original.namespace);
        assert_eq!(reloaded.language, original.language);
        assert_eq!(reloaded.issuer, original.issuer);
        assert_eq!(reloaded.issuer_client_id, original.issuer_client_id);
        assert_eq!(reloaded.name, original.name);
    }

    #[test]
    fn save_project_settings_preserves_other_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Vorpal.toml");

        // Write an initial Vorpal.toml with existing content
        let initial = r#"language = "rust"
name = "my-project"

[source]
includes = ["src", "lib"]

[source.rust]
packages = ["my-crate"]
"#;
        fs::write(&path, initial).unwrap();

        // Save settings into the same file
        let settings = Settings {
            registry: Some("https://prod.example.com".to_string()),
            ..Default::default()
        };
        save_project_settings(&path, &settings).unwrap();

        // Re-read and verify other content survived
        let contents = fs::read_to_string(&path).unwrap();
        let table: toml::Table = toml::from_str(&contents).unwrap();

        // Top-level keys preserved
        assert_eq!(
            table.get("language").and_then(|v| v.as_str()),
            Some("rust")
        );
        assert_eq!(
            table.get("name").and_then(|v| v.as_str()),
            Some("my-project")
        );

        // [source] table preserved
        let source = table.get("source").expect("[source] should exist");
        let includes = source
            .get("includes")
            .and_then(|v| v.as_array())
            .expect("includes should be an array");
        assert_eq!(includes.len(), 2);

        // [source.rust] preserved
        let rust = source
            .get("rust")
            .expect("[source.rust] should exist");
        let packages = rust
            .get("packages")
            .and_then(|v| v.as_array())
            .expect("packages should be an array");
        assert_eq!(packages.len(), 1);

        // [settings] section present with correct value
        let settings_table = table.get("settings").expect("[settings] should exist");
        assert_eq!(
            settings_table
                .get("registry")
                .and_then(|v| v.as_str()),
            Some("https://prod.example.com")
        );

        // Reload via load_project_settings to confirm
        let reloaded = load_project_settings(&path).unwrap();
        assert_eq!(
            reloaded.registry.as_deref(),
            Some("https://prod.example.com")
        );
    }

    #[test]
    fn save_project_settings_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new_dir").join("Vorpal.toml");

        let settings = Settings {
            language: Some("python".to_string()),
            ..Default::default()
        };
        save_project_settings(&path, &settings).unwrap();
        assert!(path.exists());

        let reloaded = load_project_settings(&path).unwrap();
        assert_eq!(reloaded.language.as_deref(), Some("python"));
    }

    #[test]
    fn resolve_settings_precedence_with_files() {
        let dir = tempfile::tempdir().unwrap();

        // Create user settings
        let user_path = dir.path().join("user").join("settings.json");
        let user_settings = Settings {
            language: Some("go".to_string()),
            namespace: Some("user-ns".to_string()),
            ..Default::default()
        };
        save_user_settings(&user_path, &user_settings).unwrap();

        // Create project settings
        let project_path = dir.path().join("Vorpal.toml");
        let project_settings = Settings {
            language: Some("python".to_string()),
            ..Default::default()
        };
        save_project_settings(&project_path, &project_settings).unwrap();

        // Load and resolve manually (not using resolve_settings() since
        // that depends on cwd and env vars)
        let defaults = Settings::defaults();
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        // Project overrides user for language
        assert_eq!(resolved.language.value, "python");
        assert_eq!(resolved.language.source, SettingsSource::Project);

        // User overrides default for namespace
        assert_eq!(resolved.namespace.value, "user-ns");
        assert_eq!(resolved.namespace.source, SettingsSource::User);

        // Default used for registry
        assert_eq!(resolved.registry.value, "unix:///var/lib/vorpal/vorpal.sock");
        assert_eq!(resolved.registry.source, SettingsSource::Default);

        // Default used for issuer
        assert_eq!(resolved.issuer.source, SettingsSource::Default);

        // Default used for issuer_client_id
        assert_eq!(resolved.issuer_client_id.source, SettingsSource::Default);
    }

    #[test]
    fn get_user_settings_path_behavior() {
        // This test exercises both the default and override paths of
        // get_user_settings_path(). We combine them into one test to avoid
        // env var race conditions when tests run in parallel.

        // Save and clear override
        let prev = std::env::var("VORPAL_USER_CONFIG_DIR").ok();
        std::env::remove_var("VORPAL_USER_CONFIG_DIR");

        // Without the override, path should end with .vorpal/settings.json
        let default_path = get_user_settings_path();
        assert!(
            default_path.ends_with(".vorpal/settings.json"),
            "expected path ending with .vorpal/settings.json, got: {}",
            default_path.display()
        );

        // With the override, path should use the custom directory
        std::env::set_var("VORPAL_USER_CONFIG_DIR", "/tmp/test-vorpal-config");
        let override_path = get_user_settings_path();
        assert_eq!(
            override_path,
            PathBuf::from("/tmp/test-vorpal-config/settings.json")
        );

        // Restore
        match prev {
            Some(val) => std::env::set_var("VORPAL_USER_CONFIG_DIR", val),
            None => std::env::remove_var("VORPAL_USER_CONFIG_DIR"),
        }
    }
}
