use anyhow::{anyhow, Result};
use std::{collections::HashMap, path::Path};
use tokio::{fs::create_dir_all, process::Command};

#[allow(clippy::too_many_arguments)]
pub async fn build(
    env_vars: HashMap<String, String>,
    home_path: &Path,
    package_path: &Path,
    package_paths: Vec<String>,
    sandbox_package_path: &Path,
    script_path: &Path,
    source_path: &Path,
) -> Result<Command> {
    create_dir_all(&home_path)
        .await
        .map_err(|err| anyhow!("failed to create home directory: {:?}", err))?;

    let bin_path = sandbox_package_path.join("bin");
    let etc_path = sandbox_package_path.join("etc");
    let lib_path = sandbox_package_path.join("lib");
    let lib64_path = sandbox_package_path.join("lib64");
    let sbin_path = sandbox_package_path.join("sbin");
    let share_path = sandbox_package_path.join("share");
    let usr_path = sandbox_package_path.join("usr");

    let mut command_args = vec![
        vec!["--unshare-all"],
        vec!["--share-net"],
        vec!["--clearenv"],
        vec!["--chdir", source_path.to_str().unwrap()],
        vec!["--gid", "1000"],
        vec!["--uid", "1000"],
        vec!["--dev", "/dev"],
        vec!["--proc", "/proc"],
        vec!["--tmpfs", "/tmp"],
        vec!["--ro-bind-try", bin_path.to_str().unwrap(), "/bin"],
        vec!["--ro-bind-try", etc_path.to_str().unwrap(), "/etc"],
        vec!["--ro-bind-try", lib64_path.to_str().unwrap(), "/lib64"],
        vec!["--ro-bind-try", lib_path.to_str().unwrap(), "/lib"],
        vec!["--ro-bind-try", sbin_path.to_str().unwrap(), "/sbin"],
        vec!["--ro-bind-try", share_path.to_str().unwrap(), "/share"],
        vec!["--ro-bind-try", usr_path.to_str().unwrap(), "/usr"],
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
        vec![
            "--ro-bind",
            script_path.to_str().unwrap(),
            script_path.to_str().unwrap(),
        ],
    ];

    // Add package paths to command

    for package in &package_paths {
        command_args.push(vec!["--ro-bind", package.as_str(), package.as_str()]);
    }

    // Add environment variables to command

    command_args.push(vec!["--setenv", "HOME", home_path.to_str().unwrap()]);

    for (key, value) in &env_vars {
        command_args.push(vec!["--setenv", key, value]);
    }

    // Add script path to command

    command_args.push(vec![script_path.to_str().unwrap()]);

    // Create command

    let mut command = Command::new("bwrap");

    command.args(command_args.iter().flatten());

    println!("command: {:?}", command);

    Ok(command)
}
