use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[allow(clippy::too_many_arguments)]
pub async fn build(
    sandbox_bin_paths: Vec<String>,
    sandbox_env: HashMap<String, String>,
    sandbox_home_dir_path: &Path,
    sandbox_output_dir_path: &Path,
    sandbox_package_paths: &[String],
    sandbox_script_package_path: &Path,
    sandbox_script_path: &Path,
    sandbox_source_dir_path: &Path,
    sandbox_stdenv_dir_path: Option<PathBuf>,
) -> Result<Command> {
    let stdenv_dir_path = sandbox_stdenv_dir_path.expect("failed to get stdenv path");

    let mut env_path = "/bin:/sbin".to_string();

    if !sandbox_bin_paths.is_empty() {
        env_path = format!("{}:{}", sandbox_bin_paths.join(":"), env_path);
    }

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
            sandbox_home_dir_path.to_str().unwrap(),
            sandbox_home_dir_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            sandbox_output_dir_path.to_str().unwrap(),
            sandbox_output_dir_path.to_str().unwrap(),
        ],
        vec![
            "--bind",
            sandbox_source_dir_path.to_str().unwrap(),
            sandbox_source_dir_path.to_str().unwrap(),
        ],
        vec!["--chdir", sandbox_source_dir_path.to_str().unwrap()],
        vec!["--clearenv"],
        vec!["--dev", "/dev"],
        vec!["--proc", "/proc"],
        vec![
            "--ro-bind",
            sandbox_script_package_path.to_str().unwrap(),
            sandbox_script_package_path.to_str().unwrap(),
        ],
        vec![
            "--ro-bind",
            sandbox_script_path.to_str().unwrap(),
            sandbox_script_path.to_str().unwrap(),
        ],
        vec!["--ro-bind", bin_dir_path.to_str().unwrap(), "/bin"],
        vec![
            "--ro-bind",
            stdenv_dir_path.to_str().unwrap(),
            stdenv_dir_path.to_str().unwrap(),
        ],
        vec!["--ro-bind", etc_dir_path.to_str().unwrap(), "/etc"],
        vec!["--ro-bind-try", lib64_dir_path.to_str().unwrap(), "/lib64"],
        vec!["--ro-bind", lib_dir_path.to_str().unwrap(), "/lib"],
        vec!["--ro-bind", libexec_dir_path.to_str().unwrap(), "/libexec"],
        vec!["--ro-bind", sbin_dir_path.to_str().unwrap(), "/sbin"],
        vec!["--ro-bind", share_dir_path.to_str().unwrap(), "/share"],
        vec!["--ro-bind", usr_dir_path.to_str().unwrap(), "/usr"],
        vec!["--setenv", "HOME", sandbox_home_dir_path.to_str().unwrap()],
        vec!["--setenv", "LD_LIBRARY_PATH", "/lib:/lib64"],
        vec!["--setenv", "PATH", env_path.as_str()],
        vec!["--tmpfs", "/tmp"],
        vec!["--unshare-all"],
        vec!["--share-net"],
    ];

    for package_path in sandbox_package_paths.iter() {
        build_command_args.push(vec![
            "--ro-bind",
            package_path.as_str(),
            package_path.as_str(),
        ]);
    }

    let mut env_vars_strings = Vec::new();

    for (key, value) in sandbox_env.clone().into_iter() {
        let key_str = key.to_string();
        let value_str = value.to_string();
        env_vars_strings.push((key_str, value_str));
    }

    for (key, value) in &env_vars_strings {
        build_command_args.push(vec!["--setenv", key, value]);
    }

    build_command_args.push(vec![sandbox_script_path.to_str().unwrap()]);

    build_command_args.push(vec![sandbox_script_package_path.to_str().unwrap()]);

    let mut command = Command::new("bwrap");

    let mut command_args = vec![];

    for build_command_args in build_command_args {
        command_args.extend(build_command_args);
    }

    command.args(command_args.clone());

    Ok(command)
}
