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
