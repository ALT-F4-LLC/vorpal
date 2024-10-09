use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use tokio::{fs::create_dir_all, process::Command};

#[allow(clippy::too_many_arguments)]
pub async fn build(
    build_bin_paths: Vec<String>,
    build_env: HashMap<String, String>,
    build_path: &Path,
    build_packages: Vec<String>,
) -> Result<Command> {
    let home_path = build_path.join("home");

    create_dir_all(&home_path)
        .await
        .map_err(|err| anyhow!("failed to create home directory: {:?}", err))?;

    let source_path = build_path.join("source");

    if !source_path.exists() {
        return Err(anyhow::anyhow!("build 'source' not found"));
    }

    let env_vars_path = build_bin_paths.join(":");

    let mut command_args = vec![
        vec![
            "--bind",
            build_path.to_str().unwrap(),
            build_path.to_str().unwrap(),
        ],
        vec!["--chdir", source_path.to_str().unwrap()],
        vec!["--clearenv"],
        vec!["--dev", "/dev"],
        vec!["--proc", "/proc"],
        vec!["--setenv", "HOME", home_path.to_str().unwrap()],
        vec!["--setenv", "PATH", env_vars_path.as_str()],
        vec!["--tmpfs", "/tmp"],
        vec!["--unshare-all"],
        vec!["--share-net"],
    ];

    for package in &build_packages {
        command_args.push(vec!["--ro-bind", package.as_str(), package.as_str()]);
    }

    for (key, value) in &build_env {
        command_args.push(vec!["--setenv", key, value]);
    }

    let script_path = build_path.join("package.sh");

    if !script_path.exists() {
        return Err(anyhow!("build 'package.sh' not found"));
    }

    command_args.push(vec![script_path.to_str().unwrap()]);

    let mut command = Command::new("bwrap");

    command.args(command_args.iter().flatten());

    Ok(command)
}
