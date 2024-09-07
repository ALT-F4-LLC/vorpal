use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;
use vorpal_store::paths::{get_sandbox_dir_path, get_store_dir_path};

pub async fn build(
    bin_paths: Vec<String>,
    env_var: HashMap<String, String>,
    sandbox_home_dir_path: &Path,
    sandbox_script_path: &Path,
    sandbox_source_dir_path: &Path,
    sandbox_stdenv_dir_path: &Path,
) -> Result<Command> {
    let sandbox_dir_path = get_sandbox_dir_path();
    let store_dir_path = get_store_dir_path();

    let mut env_var_path = format!("{}/bin", sandbox_stdenv_dir_path.to_str().unwrap());

    if !bin_paths.is_empty() {
        env_var_path = format!("{}:{}", bin_paths.join(":"), env_var_path);
    }

    let mut build_command_args = vec![
        "--dev",
        "/dev",
        "--bind",
        sandbox_dir_path.to_str().unwrap(),
        sandbox_dir_path.to_str().unwrap(),
        "--chdir",
        sandbox_source_dir_path.to_str().unwrap(),
        "--clearenv",
        "--proc",
        "/proc",
        "--ro-bind",
        "/etc/resolv.conf",
        "/etc/resolv.conf",
        "--ro-bind",
        "/etc/ssl/certs/",
        "/etc/ssl/certs/",
        "--ro-bind",
        "/lib",
        "/lib",
        "--ro-bind",
        "/usr/lib",
        "/usr/lib",
        "--ro-bind",
        store_dir_path.to_str().unwrap(),
        store_dir_path.to_str().unwrap(),
        "--setenv",
        "HOME",
        sandbox_home_dir_path.to_str().unwrap(),
        "--setenv",
        "PATH",
        env_var_path.as_str(),
        "--tmpfs",
        "/tmp",
        "--unshare-all",
        "--share-net",
    ];

    let mut env_vars_strings = Vec::new();

    for (key, value) in env_var.clone().into_iter() {
        let key_str = key.to_string();
        let value_str = value.to_string();
        env_vars_strings.push((key_str, value_str));
    }

    for (key, value) in &env_vars_strings {
        build_command_args.push("--setenv");
        build_command_args.push(key);
        build_command_args.push(value);
    }

    build_command_args.push(sandbox_script_path.to_str().unwrap());

    println!("build_command_args: {:?}", build_command_args);

    let mut sandbox_command = Command::new("/usr/bin/bwrap");

    sandbox_command.args(build_command_args);

    Ok(sandbox_command)
}
