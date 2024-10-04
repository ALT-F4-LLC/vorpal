use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use tokio::process::Command;

pub async fn build(
    build_bin_paths: Vec<String>,
    build_env: HashMap<String, String>,
    build_path: &Path,
    sandbox_path: &Path,
) -> Result<Command> {
    let sandbox_script_path = sandbox_path.join("sandbox.sh");

    if !sandbox_script_path.exists() {
        return Err(anyhow::anyhow!("sandbox 'sandbox.sh' not found"));
    }

    let mut command = Command::new(sandbox_script_path);

    let build_script_path = build_path.join("package.sh");

    if !build_script_path.exists() {
        return Err(anyhow::anyhow!("build 'package.sh' not found"));
    }

    command.args([build_script_path.to_str().unwrap()]);

    let build_source_path = build_path.join("source");

    if !build_source_path.exists() {
        return Err(anyhow::anyhow!("build 'source' not found"));
    }

    command.current_dir(build_source_path);

    for (key, value) in build_env.clone().into_iter() {
        command.env(key, value);
    }

    let mut path = env::var("PATH").unwrap_or_default();

    if !build_bin_paths.is_empty() {
        path = format!("{}:{}", build_bin_paths.join(":"), path);
    }

    command.env("PATH", path);

    Ok(command)
}
