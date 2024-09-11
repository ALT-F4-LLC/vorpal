use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub async fn build(
    bin_paths: Vec<String>,
    env_var: HashMap<String, String>,
    script_path: &Path,
    source_dir_path: &PathBuf,
) -> Result<Command> {
    let mut sandbox_command = Command::new("/bin/bash");

    sandbox_command.args([script_path.to_str().unwrap()]);

    sandbox_command.current_dir(source_dir_path);

    for (key, value) in env_var.clone().into_iter() {
        sandbox_command.env(key, value);
    }

    let path_default = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin";

    sandbox_command.env("PATH", path_default);

    if !bin_paths.is_empty() {
        let path = format!("{}:{}", bin_paths.join(":"), path_default);

        sandbox_command.env("PATH", path);
    }

    Ok(sandbox_command)
}
