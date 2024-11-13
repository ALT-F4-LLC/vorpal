use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

pub mod profile;

pub async fn build(
    env_vars: HashMap<String, String>,
    profile_path: &Path,
    script_path: &Path,
    source_path: &Path,
) -> Result<Command> {
    let command_args = [
        "-f",
        profile_path.to_str().unwrap(),
        script_path.to_str().unwrap(),
    ];

    let mut command = Command::new("sandbox-exec");

    command.args(command_args);

    command.current_dir(source_path);

    for (key, value) in env_vars.clone().into_iter() {
        command.env(key, value);
    }

    Ok(command)
}