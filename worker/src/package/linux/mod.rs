use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use tokio::{fs::create_dir_all, process::Command};

#[allow(clippy::too_many_arguments)]
pub async fn build(
    build_bin_paths: Vec<String>,
    build_env: HashMap<String, String>,
    build_path: &Path,
    sandbox_packages: &[String],
    sandbox_path: &Path,
) -> Result<Command> {
    let mut env_path = "/bin:/sbin".to_string();

    if !build_bin_paths.is_empty() {
        env_path = format!("{}:{}", build_bin_paths.join(":"), env_path);
    }

    let build_source_path = build_path.join("source");

    if !build_source_path.exists() {
        return Err(anyhow::anyhow!("build 'source' not found"));
    }

    let build_home_path = build_path.join("home");

    create_dir_all(&build_home_path)
        .await
        .map_err(|err| anyhow!("failed to create home directory: {:?}", err))?;

    let bin_path = sandbox_path.join("bin");
    let etc_path = sandbox_path.join("etc");
    let lib64_path = sandbox_path.join("lib64");
    let lib_path = sandbox_path.join("lib");
    let libexec_path = sandbox_path.join("libexec");
    let share_path = sandbox_path.join("share");
    let sbin_path = sandbox_path.join("sbin");
    let usr_path = sandbox_path.join("usr");

    let mut build_command_args = vec![
        vec![
            "--bind",
            build_path.to_str().unwrap(),
            build_path.to_str().unwrap(),
        ],
        vec!["--chdir", build_source_path.to_str().unwrap()],
        vec!["--clearenv"],
        vec!["--dev", "/dev"],
        vec!["--proc", "/proc"],
        vec![
            "--ro-bind",
            sandbox_path.to_str().unwrap(),
            sandbox_path.to_str().unwrap(),
        ],
        vec!["--ro-bind", bin_path.to_str().unwrap(), "/bin"],
        vec!["--ro-bind", etc_path.to_str().unwrap(), "/etc"],
        vec!["--ro-bind-try", lib64_path.to_str().unwrap(), "/lib64"],
        vec!["--ro-bind", lib_path.to_str().unwrap(), "/lib"],
        vec!["--ro-bind", libexec_path.to_str().unwrap(), "/libexec"],
        vec!["--ro-bind", sbin_path.to_str().unwrap(), "/sbin"],
        vec!["--ro-bind", share_path.to_str().unwrap(), "/share"],
        vec!["--ro-bind", usr_path.to_str().unwrap(), "/usr"],
        vec!["--setenv", "HOME", build_home_path.to_str().unwrap()],
        vec!["--setenv", "LD_LIBRARY_PATH", "/lib:/lib64"],
        vec!["--setenv", "PATH", env_path.as_str()],
        vec!["--tmpfs", "/tmp"],
        vec!["--unshare-all"],
        vec!["--share-net"],
    ];

    for package_path in sandbox_packages.iter() {
        build_command_args.push(vec![
            "--ro-bind",
            package_path.as_str(),
            package_path.as_str(),
        ]);
    }

    let mut env_vars_strings = Vec::new();

    for (key, value) in build_env.clone().into_iter() {
        let key_str = key.to_string();
        let value_str = value.to_string();
        env_vars_strings.push((key_str, value_str));
    }

    for (key, value) in &env_vars_strings {
        build_command_args.push(vec!["--setenv", key, value]);
    }

    let sandbox_script_path = sandbox_path.join("sandbox.sh");

    if !sandbox_script_path.exists() {
        return Err(anyhow!("sandbox 'sandbox.sh' not found"));
    }

    build_command_args.push(vec![sandbox_script_path.to_str().unwrap()]);

    let build_script_path = build_path.join("package.sh");

    if !build_script_path.exists() {
        return Err(anyhow!("build 'package.sh' not found"));
    }

    build_command_args.push(vec![build_script_path.to_str().unwrap()]);

    let mut command = Command::new("bwrap");

    let mut command_args = vec![];

    for build_command_args in build_command_args {
        command_args.extend(build_command_args);
    }

    command.args(command_args.clone());

    Ok(command)
}
