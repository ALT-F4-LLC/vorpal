use anyhow::{anyhow, Result};
use clap::Subcommand;
use std::path::PathBuf;

use crate::command::settings::{
    get_user_settings_path, load_project_settings, load_user_settings, resolve_settings,
    save_project_settings, save_user_settings, Settings,
};

/// Subcommands for `vorpal config`.
#[derive(Subcommand)]
pub enum ConfigAction {
    /// Set a configuration value
    Set {
        /// Configuration key (e.g., registry, namespace, language)
        key: String,
        /// Value to set
        value: String,
    },
    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },
    /// Show all configuration values with their sources
    Show,
}

/// Handle `vorpal config set <key> <value>`.
///
/// When `user_level` is true, writes to `~/.vorpal/settings.json`.
/// Otherwise, writes to `./Vorpal.toml` under the `[settings]` section.
pub fn handle_set(key: &str, value: &str, user_level: bool) -> Result<()> {
    if user_level {
        let path = get_user_settings_path();
        let mut settings = load_user_settings(&path)?;
        settings
            .set_by_name(key, value.to_string())
            .map_err(|e| anyhow!("{}", e))?;
        save_user_settings(&path, &settings)?;
        println!("Set {} = {} (user: {})", key, value, path.display());
    } else {
        let path = PathBuf::from("Vorpal.toml");
        let mut settings = load_project_settings(&path)?;
        settings
            .set_by_name(key, value.to_string())
            .map_err(|e| anyhow!("{}", e))?;
        save_project_settings(&path, &settings)?;
        println!("Set {} = {} (project: {})", key, value, path.display());
    }
    Ok(())
}

/// Handle `vorpal config get <key>`.
///
/// Resolves the value across all layers and prints it along with the source.
pub fn handle_get(key: &str, _user_level: bool) -> Result<()> {
    // Validate the key name before resolving
    if !Settings::field_names().contains(&key) {
        return Err(anyhow!(
            "unknown setting key '{}'. Valid keys: {}",
            key,
            Settings::field_names().join(", ")
        ));
    }

    let resolved = resolve_settings()?;
    match resolved.get_by_name(key) {
        Some(rv) => {
            println!("{} = {} ({})", key, rv.value, rv.source);
            Ok(())
        }
        None => Err(anyhow!(
            "unknown setting key '{}'. Valid keys: {}",
            key,
            Settings::field_names().join(", ")
        )),
    }
}

/// Handle `vorpal config show`.
///
/// Prints all configuration values in a table with KEY, VALUE, and SOURCE columns.
pub fn handle_show() -> Result<()> {
    let resolved = resolve_settings()?;
    let names = Settings::field_names();

    // Collect rows to compute column widths
    let mut rows: Vec<(&str, String, String)> = Vec::with_capacity(names.len());
    for &name in names {
        if let Some(rv) = resolved.get_by_name(name) {
            rows.push((name, rv.value.clone(), rv.source.to_string()));
        }
    }

    // Column headers
    let header_key = "KEY";
    let header_value = "VALUE";
    let header_source = "SOURCE";

    let key_width = rows
        .iter()
        .map(|(k, _, _)| k.len())
        .max()
        .unwrap_or(0)
        .max(header_key.len());
    let value_width = rows
        .iter()
        .map(|(_, v, _)| v.len())
        .max()
        .unwrap_or(0)
        .max(header_value.len());
    let source_width = rows
        .iter()
        .map(|(_, _, s)| s.len())
        .max()
        .unwrap_or(0)
        .max(header_source.len());

    // Print header
    println!(
        "{:<key_width$}  {:<value_width$}  {:<source_width$}",
        header_key, header_value, header_source,
    );
    println!(
        "{:<key_width$}  {:<value_width$}  {:<source_width$}",
        "-".repeat(key_width),
        "-".repeat(value_width),
        "-".repeat(source_width),
    );

    // Print rows
    for (key, value, source) in &rows {
        println!(
            "{:<key_width$}  {:<value_width$}  {:<source_width$}",
            key, value, source,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::settings::{
        load_project_settings, load_user_settings, save_project_settings, save_user_settings,
        ResolvedSettings, SettingsSource,
    };
    use std::fs;

    // -----------------------------------------------------------------------
    // Original handler tests
    // -----------------------------------------------------------------------

    #[test]
    fn handle_set_project_level() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Vorpal.toml");

        // Write a minimal Vorpal.toml
        fs::write(&path, "").unwrap();

        // We can't easily call handle_set because it uses a hardcoded path,
        // but we can test the underlying logic directly.
        let mut settings = Settings::default();
        settings
            .set_by_name("language", "go".to_string())
            .expect("set language");
        save_project_settings(&path, &settings).unwrap();

        let reloaded = load_project_settings(&path).unwrap();
        assert_eq!(reloaded.language.as_deref(), Some("go"));
    }

    #[test]
    fn handle_set_user_level() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");

        let mut settings = Settings::default();
        settings
            .set_by_name("registry", "https://example.com".to_string())
            .expect("set registry");
        save_user_settings(&path, &settings).unwrap();

        let reloaded = load_user_settings(&path).unwrap();
        assert_eq!(reloaded.registry.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn handle_get_invalid_key_returns_error() {
        let result = handle_get("nonexistent", false);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("unknown setting key"));
        assert!(err_msg.contains("nonexistent"));
    }

    // -----------------------------------------------------------------------
    // Integration tests: full config lifecycle (DKT-6)
    // -----------------------------------------------------------------------

    /// Scenario 1: Fresh state with no config files -- all values resolve to
    /// built-in defaults.
    #[test]
    fn lifecycle_fresh_state_resolves_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = dir.path().join("user").join("settings.json");
        let project_path = dir.path().join("Vorpal.toml");

        // Neither file exists on disk
        assert!(!user_path.exists());
        assert!(!project_path.exists());

        let defaults = Settings::defaults();
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        // Every field should resolve to the built-in default
        for &name in Settings::field_names() {
            let rv = resolved.get_by_name(name).expect("field should exist");
            assert_eq!(
                rv.source,
                SettingsSource::Default,
                "field '{}' should have source=default, got {:?}",
                name,
                rv.source
            );
            // The value should match Settings::defaults()
            let expected = defaults
                .get_by_name(name)
                .expect("defaults should have all fields");
            assert_eq!(
                rv.value, *expected,
                "field '{}' value mismatch",
                name
            );
        }
    }

    /// Scenario 2: Set a value at user level -- verify get shows it with
    /// source=user.
    #[test]
    fn lifecycle_set_user_value_shows_source_user() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = dir.path().join("user").join("settings.json");
        let project_path = dir.path().join("Vorpal.toml");

        // Set language at user level
        let mut user_settings = Settings::default();
        user_settings
            .set_by_name("language", "go".to_string())
            .unwrap();
        save_user_settings(&user_path, &user_settings).unwrap();

        // Resolve
        let defaults = Settings::defaults();
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        assert_eq!(resolved.language.value, "go");
        assert_eq!(resolved.language.source, SettingsSource::User);

        // Other fields still at default
        assert_eq!(resolved.registry.source, SettingsSource::Default);
        assert_eq!(resolved.namespace.source, SettingsSource::Default);
    }

    /// Scenario 3: Set a different value at project level for the same key --
    /// verify project value wins (higher precedence).
    #[test]
    fn lifecycle_project_overrides_user() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = dir.path().join("user").join("settings.json");
        let project_path = dir.path().join("Vorpal.toml");

        // Set language=go at user level
        let mut user_settings = Settings::default();
        user_settings
            .set_by_name("language", "go".to_string())
            .unwrap();
        save_user_settings(&user_path, &user_settings).unwrap();

        // Set language=python at project level
        let mut project_settings = Settings::default();
        project_settings
            .set_by_name("language", "python".to_string())
            .unwrap();
        save_project_settings(&project_path, &project_settings).unwrap();

        // Resolve
        let defaults = Settings::defaults();
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        // Project wins over user
        assert_eq!(resolved.language.value, "python");
        assert_eq!(resolved.language.source, SettingsSource::Project);
    }

    /// Scenario 4: Remove the project-level value -- verify fallback to user
    /// value.
    #[test]
    fn lifecycle_remove_project_falls_back_to_user() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = dir.path().join("user").join("settings.json");
        let project_path = dir.path().join("Vorpal.toml");

        // Set language=go at user level
        let mut user_settings = Settings::default();
        user_settings
            .set_by_name("language", "go".to_string())
            .unwrap();
        save_user_settings(&user_path, &user_settings).unwrap();

        // Set language=python at project level
        let mut project_settings = Settings::default();
        project_settings
            .set_by_name("language", "python".to_string())
            .unwrap();
        save_project_settings(&project_path, &project_settings).unwrap();

        // Verify project wins initially
        let defaults = Settings::defaults();
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);
        assert_eq!(resolved.language.value, "python");
        assert_eq!(resolved.language.source, SettingsSource::Project);

        // Now remove the project-level value by saving an empty Settings
        let empty_project = Settings::default();
        save_project_settings(&project_path, &empty_project).unwrap();

        // Re-resolve: should fall back to user value
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        assert_eq!(resolved.language.value, "go");
        assert_eq!(resolved.language.source, SettingsSource::User);
    }

    /// Scenario 5: Config show displays all keys -- verify all 5 settings are
    /// present with correct sources.
    #[test]
    fn lifecycle_show_displays_all_keys_with_correct_sources() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = dir.path().join("user").join("settings.json");
        let project_path = dir.path().join("Vorpal.toml");

        // User overrides namespace and issuer
        let mut user_settings = Settings::default();
        user_settings
            .set_by_name("namespace", "my-ns".to_string())
            .unwrap();
        user_settings
            .set_by_name("issuer", "https://user-auth.example.com".to_string())
            .unwrap();
        save_user_settings(&user_path, &user_settings).unwrap();

        // Project overrides issuer (overriding user) and issuer_client_id
        let mut project_settings = Settings::default();
        project_settings
            .set_by_name("issuer", "https://project-auth.example.com".to_string())
            .unwrap();
        project_settings
            .set_by_name("issuer_client_id", "project-cli".to_string())
            .unwrap();
        save_project_settings(&project_path, &project_settings).unwrap();

        // Resolve
        let defaults = Settings::defaults();
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        // Verify all 5 fields are present via get_by_name
        let expected: Vec<(&str, &str, SettingsSource)> = vec![
            (
                "registry",
                "unix:///var/lib/vorpal/vorpal.sock",
                SettingsSource::Default,
            ),
            ("namespace", "my-ns", SettingsSource::User),
            ("language", "rust", SettingsSource::Default),
            (
                "issuer",
                "https://project-auth.example.com",
                SettingsSource::Project,
            ),
            ("issuer_client_id", "project-cli", SettingsSource::Project),
            ("name", "vorpal", SettingsSource::Default),
        ];

        for (key, exp_value, exp_source) in &expected {
            let rv = resolved
                .get_by_name(key)
                .unwrap_or_else(|| panic!("resolved should contain key '{}'", key));
            assert_eq!(
                rv.value, *exp_value,
                "value mismatch for key '{}'",
                key
            );
            assert_eq!(
                rv.source, *exp_source,
                "source mismatch for key '{}'",
                key
            );
        }

        // Verify field_names covers all 6
        assert_eq!(Settings::field_names().len(), 6);
    }

    /// Scenario 6: Writing to user config creates the parent directory
    /// (e.g., ~/.vorpal/) if it does not exist.
    #[test]
    fn lifecycle_user_config_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        // Deep nested path that does not exist
        let user_dir = dir.path().join("does").join("not").join("exist");
        let user_path = user_dir.join("settings.json");

        assert!(!user_dir.exists());

        let mut settings = Settings::default();
        settings
            .set_by_name("registry", "https://new.example.com".to_string())
            .unwrap();
        save_user_settings(&user_path, &settings).unwrap();

        // Directory and file should now exist
        assert!(user_dir.exists());
        assert!(user_path.exists());

        // Verify content survived
        let reloaded = load_user_settings(&user_path).unwrap();
        assert_eq!(
            reloaded.registry.as_deref(),
            Some("https://new.example.com")
        );
    }

    /// Scenario 7: Writing to project config preserves existing Vorpal.toml
    /// content (other tables survive).
    #[test]
    fn lifecycle_project_config_preserves_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("Vorpal.toml");

        // Write an existing Vorpal.toml with language, source, and build tables
        let initial_content = r#"language = "rust"
name = "my-project"

[source]
includes = ["src", "lib"]

[build]
target = "release"
"#;
        fs::write(&project_path, initial_content).unwrap();

        // Save a settings value into the file
        let mut settings = Settings::default();
        settings
            .set_by_name("namespace", "test-ns".to_string())
            .unwrap();
        save_project_settings(&project_path, &settings).unwrap();

        // Re-read the raw TOML and verify all existing tables survived
        let contents = fs::read_to_string(&project_path).unwrap();
        let table: toml::Table = toml::from_str(&contents).unwrap();

        // Top-level keys preserved
        assert_eq!(
            table.get("language").and_then(|v| v.as_str()),
            Some("rust"),
            "top-level 'language' key should survive"
        );
        assert_eq!(
            table.get("name").and_then(|v| v.as_str()),
            Some("my-project"),
            "top-level 'name' key should survive"
        );

        // [source] table preserved
        let source = table.get("source").expect("[source] should survive");
        let includes = source
            .get("includes")
            .and_then(|v| v.as_array())
            .expect("includes should be an array");
        assert_eq!(includes.len(), 2);

        // [build] table preserved
        let build = table.get("build").expect("[build] should survive");
        assert_eq!(
            build.get("target").and_then(|v| v.as_str()),
            Some("release")
        );

        // [settings] section present with new value
        let settings_table = table.get("settings").expect("[settings] should exist");
        assert_eq!(
            settings_table
                .get("namespace")
                .and_then(|v| v.as_str()),
            Some("test-ns")
        );

        // Confirm via load_project_settings
        let reloaded = load_project_settings(&project_path).unwrap();
        assert_eq!(reloaded.namespace.as_deref(), Some("test-ns"));
    }

    /// Scenario 8: Invalid key names produce helpful error messages.
    #[test]
    fn lifecycle_invalid_key_produces_helpful_error() {
        let mut settings = Settings::default();

        // Completely bogus key
        let result = settings.set_by_name("bogus_key", "value".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("unknown setting key"),
            "error should mention 'unknown setting key', got: {}",
            err
        );
        assert!(
            err.contains("bogus_key"),
            "error should mention the bad key name, got: {}",
            err
        );
        // Error should list valid keys to help the user
        assert!(
            err.contains("registry"),
            "error should list valid keys, got: {}",
            err
        );
        assert!(
            err.contains("namespace"),
            "error should list valid keys, got: {}",
            err
        );

        // handle_get with invalid key also produces an error
        let result = handle_get("not_a_key", false);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("unknown setting key"));
        assert!(err_msg.contains("not_a_key"));

        // Empty string as key
        let result = settings.set_by_name("", "value".to_string());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("unknown setting key"));
    }

    /// Scenario 9: Round-trip -- setting a value and reading it back produces
    /// the same value for every setting key, in both user and project configs.
    #[test]
    fn lifecycle_roundtrip_all_keys() {
        let dir = tempfile::tempdir().unwrap();
        let user_path = dir.path().join("user").join("settings.json");
        let project_path = dir.path().join("Vorpal.toml");

        let test_values: Vec<(&str, &str)> = vec![
            ("registry", "https://roundtrip.example.com:443/api"),
            ("namespace", "round-trip-ns"),
            ("language", "typescript"),
            ("issuer", "https://auth.roundtrip.example.com/realms/test"),
            ("issuer_client_id", "roundtrip-client-12345"),
            ("name", "roundtrip-project"),
        ];

        // --- User config round-trip ---
        let mut user_settings = Settings::default();
        for &(key, value) in &test_values {
            user_settings
                .set_by_name(key, value.to_string())
                .unwrap_or_else(|e| panic!("set_by_name('{}') failed: {}", key, e));
        }
        save_user_settings(&user_path, &user_settings).unwrap();

        let reloaded_user = load_user_settings(&user_path).unwrap();
        for &(key, expected) in &test_values {
            let actual = reloaded_user
                .get_by_name(key)
                .unwrap_or_else(|| panic!("user config missing key '{}'", key));
            assert_eq!(
                actual, expected,
                "user round-trip mismatch for key '{}'",
                key
            );
        }

        // --- Project config round-trip ---
        let mut project_settings = Settings::default();
        for &(key, value) in &test_values {
            project_settings
                .set_by_name(key, value.to_string())
                .unwrap_or_else(|e| panic!("set_by_name('{}') failed: {}", key, e));
        }
        save_project_settings(&project_path, &project_settings).unwrap();

        let reloaded_project = load_project_settings(&project_path).unwrap();
        for &(key, expected) in &test_values {
            let actual = reloaded_project
                .get_by_name(key)
                .unwrap_or_else(|| panic!("project config missing key '{}'", key));
            assert_eq!(
                actual, expected,
                "project round-trip mismatch for key '{}'",
                key
            );
        }

        // --- Full resolution round-trip: verify resolved values match ---
        let defaults = Settings::defaults();
        let user = load_user_settings(&user_path).unwrap();
        let project = load_project_settings(&project_path).unwrap();
        let resolved = ResolvedSettings::resolve(&defaults, &user, &project);

        // Since both user and project have the same values, project wins
        for &(key, expected) in &test_values {
            let rv = resolved
                .get_by_name(key)
                .unwrap_or_else(|| panic!("resolved missing key '{}'", key));
            assert_eq!(
                rv.value, expected,
                "resolved round-trip mismatch for key '{}'",
                key
            );
            assert_eq!(
                rv.source,
                SettingsSource::Project,
                "source should be project when both layers set the key"
            );
        }
    }
}
