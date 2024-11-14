use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

pub async fn build(
    env_vars: HashMap<String, String>,
    script_path: &Path,
    source_path: &Path,
) -> Result<Command> {
    let mut command = Command::new(script_path);

    command.current_dir(source_path);

    for (key, value) in env_vars.clone().into_iter() {
        command.env(key, value);
    }

    Ok(command)
}
