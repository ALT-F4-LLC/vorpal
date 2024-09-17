use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[allow(clippy::too_many_arguments)]
pub async fn build(
    bin_paths: Vec<String>,
    env_var: HashMap<String, String>,
    home_dir_path: &Path,
    output_dir_path: &Path,
    package_paths: &[String],
    script_path: &Option<PathBuf>,
    source_dir_path: &Path,
    stdenv_dir_path: Option<PathBuf>,
) -> Result<Command> {
    let stdenv_dir_path = stdenv_dir_path.expect("failed to get stdenv path");

    if script_path.is_none() {
        bail!("script path is not provided")
    }

    let mut env_path = "/bin:/sbin".to_string();

    if !bin_paths.is_empty() {
        env_path = format!("{}:{}", bin_paths.join(":"), env_path);
    }

    let script_path = script_path.as_ref().unwrap();

    let bin_dir_path = stdenv_dir_path.join("bin");
    let etc_dir_path = stdenv_dir_path.join("etc");
    let lib64_dir_path = stdenv_dir_path.join("lib64");
    let lib_dir_path = stdenv_dir_path.join("lib");
    let libexec_dir_path = stdenv_dir_path.join("libexec");
    let share_dir_path = stdenv_dir_path.join("share");
    let sbin_dir_path = stdenv_dir_path.join("sbin");
    let usr_dir_path = stdenv_dir_path.join("usr");

    let mut build_command_args = vec![
        vec![
            "--bind",
            home_dir_path.to_str().unwrap(),
            home_dir_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            output_dir_path.to_str().unwrap(),
            output_dir_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            source_dir_path.to_str().unwrap(),
            source_dir_path.to_str().unwrap(),
        ],
        vec!["--chdir", source_dir_path.to_str().unwrap()],
        vec!["--clearenv"],
        vec!["--dev", "/dev"],
        vec!["--proc", "/proc"],
        vec![
            "--ro-bind",
            script_path.to_str().unwrap(),
            script_path.to_str().unwrap(),
        ],
        vec!["--ro-bind", bin_dir_path.to_str().unwrap(), "/bin"],
        vec![
            "--ro-bind",
            stdenv_dir_path.to_str().unwrap(),
            stdenv_dir_path.to_str().unwrap(),
        ],
        vec!["--ro-bind", etc_dir_path.to_str().unwrap(), "/etc"],
        vec!["--ro-bind", lib64_dir_path.to_str().unwrap(), "/lib64"],
        vec!["--ro-bind", lib_dir_path.to_str().unwrap(), "/lib"],
        vec!["--ro-bind", libexec_dir_path.to_str().unwrap(), "/libexec"],
        vec!["--ro-bind", sbin_dir_path.to_str().unwrap(), "/sbin"],
        vec!["--ro-bind", share_dir_path.to_str().unwrap(), "/share"],
        vec!["--ro-bind", usr_dir_path.to_str().unwrap(), "/usr"],
        vec!["--setenv", "HOME", home_dir_path.to_str().unwrap()],
        vec!["--setenv", "LD_LIBRARY_PATH", "/lib:/lib64"],
        vec!["--setenv", "PATH", env_path.as_str()],
        vec!["--tmpfs", "/tmp"],
        vec!["--unshare-all"],
        vec!["--share-net"],
    ];

    for package_path in package_paths.iter() {
        build_command_args.push(vec![
            "--ro-bind",
            package_path.as_str(),
            package_path.as_str(),
        ]);
    }

    let mut env_vars_strings = Vec::new();

    for (key, value) in env_var.clone().into_iter() {
        let key_str = key.to_string();
        let value_str = value.to_string();
        env_vars_strings.push((key_str, value_str));
    }

    for (key, value) in &env_vars_strings {
        build_command_args.push(vec!["--setenv", key, value]);
    }

    build_command_args.push(vec![script_path.to_str().unwrap()]);

    let mut command = Command::new("/usr/bin/bwrap");

    let mut command_args = vec![];

    for build_command_args in build_command_args {
        command_args.extend(build_command_args);
    }

    command.args(command_args.clone());

    Ok(command)
}
