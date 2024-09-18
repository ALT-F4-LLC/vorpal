use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tera::Tera;
use tokio::fs::write;
use tokio::process::Command;
use vorpal_store::temps::create_temp_file;

mod profile;

pub async fn build(
    sandbox_bin_paths: Vec<String>,
    sandbox_env: HashMap<String, String>,
    sandbox_script_package_path: &Path,
    sandbox_script_path: &Path,
    sandbox_source_dir_path: &PathBuf,
    sandbox_stdenv_dir_path: Option<PathBuf>,
) -> Result<Command> {
    let stdenv_dir_path = sandbox_stdenv_dir_path.expect("failed to get stdenv path");

    let profile_file_path = create_temp_file(Some("sb")).await?;

    let mut tera = Tera::default();

    tera.add_raw_template("sandbox_default", profile::SANDBOX_DEFAULT)
        .unwrap();

    let profile_file_context = tera::Context::new();

    let profile_file_data = tera
        .render("sandbox_default", &profile_file_context)
        .unwrap();

    write(&profile_file_path, profile_file_data)
        .await
        .expect("failed to write sandbox profile");

    let command_args = [
        "-f",
        profile_file_path.to_str().unwrap(),
        sandbox_script_path.to_str().unwrap(),
        sandbox_script_package_path.to_str().unwrap(),
    ];

    let mut command = Command::new("/usr/bin/sandbox-exec");

    command.args(command_args);

    command.current_dir(sandbox_source_dir_path);

    for (key, value) in sandbox_env.clone().into_iter() {
        command.env(key, value);
    }

    let mut path = format!(
        "{}:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
        stdenv_dir_path.join("bin").to_str().unwrap()
    );

    if !sandbox_bin_paths.is_empty() {
        path = format!("{}:{}", sandbox_bin_paths.join(":"), path);
    }

    command.env("PATH", path);

    Ok(command)
}
