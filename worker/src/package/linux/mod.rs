use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub async fn build(
    bin_paths: Vec<String>,
    env_var: HashMap<String, String>,
    // sandbox_package_dir_path: &PathBuf,
    sandbox_script_path: &Path,
    sandbox_source_dir_path: &PathBuf,
) -> Result<Command> {
    let build_command_args = [
        "--clearenv",
        "--ro-bind",
        "/bin/sh",
        "/bin/sh",
        // "--bind",
        // sandbox_package_dir_path.to_str().unwrap(),
        // sandbox_package_dir_path.to_str().unwrap(),
        // "--bind",
        // sandbox_source_dir_path.to_str().unwrap(),
        // sandbox_source_dir_path.to_str().unwrap(),
        // "--clearenv",
        // "--new-session",
        "--ro-bind",
        sandbox_script_path.to_str().unwrap(),
        sandbox_script_path.to_str().unwrap(),
        "--unshare-all",
        sandbox_script_path.to_str().unwrap(),
    ];

    println!("build_command_args: {:?}", build_command_args);

    let mut sandbox_command = Command::new("bwrap");

    sandbox_command.args(build_command_args);

    sandbox_command.current_dir(sandbox_source_dir_path);

    for (key, value) in env_var.clone().into_iter() {
        sandbox_command.env(key, value);
    }

    if !bin_paths.is_empty() {
        let path_default = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin";
        let path = format!("{}:{}", bin_paths.join(":"), path_default);
        sandbox_command.env("PATH", path);
    }

    Ok(sandbox_command)
}
