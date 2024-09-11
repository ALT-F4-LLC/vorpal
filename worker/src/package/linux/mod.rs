use anyhow::Result;
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
    script_path: &Path,
    source_dir_path: &Path,
    stdenv_dir_path: Option<PathBuf>,
) -> Result<Command> {
    let stdenv_dir_path = stdenv_dir_path.expect("failed to get stdenv path");

    let mut env_var_path = "/bin:/sbin".to_string();

    if !bin_paths.is_empty() {
        env_var_path = format!("{}:{}", bin_paths.join(":"), env_var_path);
    }

    let stdenv_bin_path = stdenv_dir_path.join("bin");
    let stdenv_etc_path = stdenv_dir_path.join("etc");
    let stdenv_lib64_path = stdenv_dir_path.join("lib64");
    let stdenv_lib_path = stdenv_dir_path.join("lib");
    let stdenv_libexec_path = stdenv_dir_path.join("libexec");
    let stdenv_share_path = stdenv_dir_path.join("share");
    let stdenv_sbin_path = stdenv_dir_path.join("sbin");
    let stdenv_usr_path = stdenv_dir_path.join("usr");

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
        vec!["--ro-bind", stdenv_bin_path.to_str().unwrap(), "/bin"],
        vec![
            "--ro-bind",
            stdenv_dir_path.to_str().unwrap(),
            stdenv_dir_path.to_str().unwrap(),
        ],
        vec!["--ro-bind", stdenv_etc_path.to_str().unwrap(), "/etc"],
        vec!["--ro-bind", stdenv_lib64_path.to_str().unwrap(), "/lib64"],
        vec!["--ro-bind", stdenv_lib_path.to_str().unwrap(), "/lib"],
        vec![
            "--ro-bind",
            stdenv_libexec_path.to_str().unwrap(),
            "/libexec",
        ],
        vec!["--ro-bind", stdenv_sbin_path.to_str().unwrap(), "/sbin"],
        vec!["--ro-bind", stdenv_share_path.to_str().unwrap(), "/share"],
        vec!["--ro-bind", stdenv_usr_path.to_str().unwrap(), "/usr"],
        vec!["--setenv", "HOME", home_dir_path.to_str().unwrap()],
        vec!["--setenv", "LD_LIBRARY_PATH", "/lib:/lib64"],
        vec!["--setenv", "PATH", env_var_path.as_str()],
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

    let mut sandbox_command = Command::new("/usr/bin/bwrap");

    let mut sandbox_command_args = vec![];

    for build_command_args in build_command_args {
        sandbox_command_args.extend(build_command_args);
    }

    println!("{:?}", sandbox_command_args);

    sandbox_command.args(sandbox_command_args.clone());

    Ok(sandbox_command)
}
