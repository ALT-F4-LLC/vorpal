use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

pub async fn build(
    sandbox_bin_paths: Vec<String>,
    sandbox_env_var: HashMap<String, String>,
    sandbox_script_package_path: &Path,
    sandbox_script_path: &Path,
    sandbox_source_dir_path: &Path,
) -> Result<Command> {
    let mut command = Command::new("/bin/bash");

    command.args([
        sandbox_script_path.to_str().unwrap(),
        sandbox_script_package_path.to_str().unwrap(),
    ]);

    command.current_dir(sandbox_source_dir_path);

    for (key, value) in sandbox_env_var.clone().into_iter() {
        command.env(key, value);
    }

    let mut path = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin".to_string();

    if !sandbox_bin_paths.is_empty() {
        path = format!("{}:{}", sandbox_bin_paths.join(":"), path);
    }

    command.env("PATH", path);

    Ok(command)
}
