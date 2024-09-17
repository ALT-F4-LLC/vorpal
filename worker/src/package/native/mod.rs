use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::process::Command;

pub async fn build(
    bin_paths: Vec<String>,
    env_var: HashMap<String, String>,
    script_path: &Option<PathBuf>,
    source_dir_path: &PathBuf,
) -> Result<Command> {
    if script_path.is_none() {
        bail!("script path is not provided")
    }

    let script_path = script_path.as_ref().unwrap();

    let mut command = Command::new("/bin/bash");

    command.args([script_path.to_str().unwrap()]);

    command.current_dir(source_dir_path);

    for (key, value) in env_var.clone().into_iter() {
        command.env(key, value);
    }

    let mut path = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string();

    if !bin_paths.is_empty() {
        path = format!("{}:{}", bin_paths.join(":"), path);
    }

    command.env("PATH", path);

    Ok(command)
}
