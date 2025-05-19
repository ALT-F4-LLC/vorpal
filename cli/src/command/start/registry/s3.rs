use anyhow::{bail, Result};

pub fn get_artifact_alias_key(alias: &str, system: &str) -> Result<String> {
    let alias_parts = alias.split(':').collect::<Vec<&str>>();

    if alias_parts.len() != 2 {
        bail!("invalid alias format");
    }

    let alias_dir = alias_parts[0];
    let alias_file = alias_parts[1];

    Ok(format!("artifact/alias/{system}/{alias_dir}/{alias_file}"))
}

pub fn get_artifact_archive_key(digest: &str) -> String {
    format!("artifact/archive/{digest}.tar.zst")
}

pub fn get_artifact_config_key(digest: &str) -> String {
    format!("artifact/config/{digest}.json")
}
