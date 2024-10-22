use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use tokio::{fs::create_dir_all, process::Command};

#[allow(clippy::too_many_arguments)]
pub async fn build(
    env_paths: Vec<String>,
    env_vars: HashMap<String, String>,
    home_path: &Path,
    package_path: &Path,
    package_paths: Vec<String>,
    script_path: &Path,
    source_path: &Path,
) -> Result<Command> {
    create_dir_all(&home_path)
        .await
        .map_err(|err| anyhow!("failed to create home directory: {:?}", err))?;

    let env_vars_path = env_paths.join(":");

    let mut command_args = vec![
        vec![
            "--bind",
            home_path.to_str().unwrap(),
            home_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            source_path.to_str().unwrap(),
            source_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            package_path.to_str().unwrap(),
            package_path.to_str().unwrap(),
        ],
        vec!["--chdir", source_path.to_str().unwrap()],
        vec!["--clearenv"],
        vec!["--dev", "/dev"],
        vec!["--proc", "/proc"],
        vec!["--setenv", "HOME", home_path.to_str().unwrap()],
        vec!["--setenv", "PATH", env_vars_path.as_str()],
        vec![
            "--ro-bind",
            script_path.to_str().unwrap(),
            script_path.to_str().unwrap(),
        ],
        vec!["--tmpfs", "/tmp"],
        vec!["--unshare-all"],
        vec!["--share-net"],
    ];

    for package in &package_paths {
        command_args.push(vec!["--ro-bind", package.as_str(), package.as_str()]);
    }

    for (key, value) in &env_vars {
        command_args.push(vec!["--setenv", key, value]);
    }

    command_args.push(vec![script_path.to_str().unwrap()]);

    let mut command = Command::new("bwrap");

    command.args(command_args.iter().flatten());

    Ok(command)
}
